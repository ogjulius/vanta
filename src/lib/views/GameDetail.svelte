<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import {
    ArrowLeft, Play, Square, Heart, Settings2, FolderOpen, ImagePlus, Trash2, LoaderCircle, X, Pencil, Check, CircleAlert, Clock,
  } from "@lucide/svelte";
  import Artwork from "../components/Artwork.svelte";
  import PlatformTag from "../components/PlatformTag.svelte";
  import ProfileEditor from "../components/ProfileEditor.svelte";
  import { api, assetUrl, errMsg } from "../api";
  import { app, back, confirmAction, launch, stop, toast, toastError } from "../state.svelte";
  import { effectivePlaytime, formatBytes, formatDateTime, formatDuration, relativeTime, PLATFORM_LABEL } from "../format";
  import type { Game, Session } from "../types";

  let { id }: { id: string } = $props();

  let game = $state<Game | null>(null);
  let sessions = $state<Session[]>([]);
  let loadError = $state<string | null>(null);
  let tab = $state<"overview" | "profile" | "sessions">("overview");
  let editingDesc = $state(false);
  let desc = $state("");
  let tagInput = $state("");
  let editingName = $state(false);
  let nameDraft = $state("");

  const running = $derived(game ? app.running[game.id] : undefined);

  $effect(() => {
    void app.libraryVersion;
    const gid = id;
    api
      .getGame(gid)
      .then((g) => {
        game = g;
        loadError = null;
      })
      .catch((e) => (loadError = errMsg(e)));
    api.gameSessions(gid).then((s) => (sessions = s)).catch(() => {});
  });

  $effect(() => {
    void id;
    tab = "overview";
    editingDesc = false;
    editingName = false;
  });

  async function toggleFavorite() {
    if (!game) return;
    try {
      game = await api.updateGame(game.id, { favorite: !game.favorite });
    } catch (e) {
      toastError("Couldn't update favorite", e);
    }
  }

  async function saveDesc() {
    if (!game) return;
    try {
      game = await api.updateGame(game.id, { description: desc });
      editingDesc = false;
    } catch (e) {
      toastError("Couldn't save description", e);
    }
  }

  async function saveName() {
    if (!game || !nameDraft.trim()) return;
    try {
      game = await api.updateGame(game.id, { name: nameDraft });
      editingName = false;
    } catch (e) {
      toastError("Couldn't rename", e);
    }
  }

  async function setTags(tags: string[]) {
    if (!game) return;
    try {
      game.tags = await api.setTags(game.id, tags);
    } catch (e) {
      toastError("Couldn't update tags", e);
    }
  }

  function addTag() {
    const t = tagInput.trim().replace(/^#/, "");
    if (!t || !game) return;
    setTags([...game.tags, t]);
    tagInput = "";
  }

  async function changeCover() {
    if (!game) return;
    const p = await openDialog({ multiple: false, title: "Choose cover art", filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp"] }] });
    if (typeof p !== "string") return;
    try {
      game = await api.setCustomCover(game.id, p);
      toast("success", "Cover updated");
    } catch (e) {
      toastError("Couldn't set cover", e);
    }
  }

  async function remove() {
    if (!game) return;
    const ok = await confirmAction({
      title: `Remove ${game.name}?`,
      message:
        game.source === "manual"
          ? "The game will be removed from VANTA along with its profile and history. Your game files are not touched."
          : `The game will be hidden from your library and won't be re-imported by future scans. Nothing is uninstalled — it stays in ${PLATFORM_LABEL[game.platform]}. You can restore it from Settings → Library.`,
      confirmLabel: "Remove from library",
      danger: true,
    });
    if (!ok) return;
    try {
      await api.removeGame(game.id);
      toast("success", "Removed from library", game.name);
      back();
    } catch (e) {
      toastError("Couldn't remove game", e);
    }
  }

  const statusLabel: Record<Session["status"], string> = { completed: "Completed", failed: "Failed", interrupted: "Interrupted", running: "Running" };
</script>

{#if loadError}
  <div class="flex h-full flex-col items-center justify-center gap-4 text-center">
    <CircleAlert size={28} class="text-red-400" />
    <p class="text-sm text-dim">{loadError}</p>
    <button class="btn" onclick={back}><ArrowLeft size={15} /> Back</button>
  </div>
{:else if !game}
  <div class="skeleton h-[420px] w-full"></div>
{:else}
  {@const g = game}
  {@const playtime = effectivePlaytime(g)}
  <div class="pb-16">
    <!-- Backdrop -->
    <section class="relative h-[400px] w-full overflow-hidden">
      <Artwork name={g.name} path={g.hero ?? g.cover} kind="hero" class="absolute inset-0 h-full w-full" eager />
      <div class="pointer-events-none absolute inset-0 bg-gradient-to-t from-bg via-bg/60 to-bg/10"></div>
      <div class="pointer-events-none absolute inset-0 bg-gradient-to-r from-bg/80 to-transparent"></div>
      <button class="btn glass absolute left-[var(--pad)] top-6 h-9" onclick={back}><ArrowLeft size={15} /> Back</button>
    </section>

    <div class="relative mx-auto -mt-[220px] max-w-[1400px] px-[var(--pad)]">
      <div class="flex gap-9">
        <!-- Cover -->
        <div class="rise w-[220px] shrink-0">
          <div class="group relative overflow-hidden rounded-xl shadow-2xl shadow-black/70 ring-1 ring-white/10">
            <Artwork name={g.name} path={g.cover} platform={g.platform} class="aspect-[2/3] w-full" eager />
            <button class="absolute inset-0 flex items-center justify-center gap-2 bg-black/60 text-xs font-medium opacity-0 t transition-opacity group-hover:opacity-100" onclick={changeCover}>
              <ImagePlus size={15} /> Change cover
            </button>
          </div>
        </div>

        <!-- Title + actions -->
        <div class="rise min-w-0 flex-1 pt-[96px]" style="animation-delay:50ms">
          <div class="mb-3 flex items-center gap-3">
            <PlatformTag platform={g.platform} size="md" />
            <span class="h-1 w-1 rounded-full bg-faint"></span>
            <span class="text-xs {g.installed ? 'text-emerald-400/90' : 'text-faint'}">{g.installed ? "Installed" : "Not installed"}</span>
            {#if g.source === "manual"}<span class="h-1 w-1 rounded-full bg-faint"></span><span class="text-xs text-faint">Added manually</span>{/if}
          </div>
          {#if editingName}
            <div class="mb-6 flex max-w-xl items-center gap-2">
              <input class="field font-display !text-2xl !font-semibold" bind:value={nameDraft} onkeydown={(e) => (e.key === "Enter" ? saveName() : e.key === "Escape" && (editingName = false))} />
              <button class="btn btn-icon" onclick={saveName} aria-label="Save name"><Check size={15} /></button>
              <button class="btn btn-ghost btn-icon" onclick={() => (editingName = false)} aria-label="Cancel"><X size={15} /></button>
            </div>
          {:else if g.logo}
            <div class="group mb-6 flex items-end gap-3">
              <img src={assetUrl(g.logo)} alt={g.name} class="max-h-[110px] max-w-[460px] object-contain object-left drop-shadow-2xl" draggable="false" />
            </div>
          {:else}
            <div class="group mb-6 flex items-center gap-3">
              <h1 class="font-display text-[40px] font-bold leading-tight">{g.name}</h1>
              <button class="rounded p-1.5 text-faint opacity-0 hover:text-text group-hover:opacity-100" onclick={() => ((nameDraft = g.name), (editingName = true))} title="Rename"><Pencil size={15} /></button>
            </div>
          {/if}

          <div class="flex flex-wrap items-center gap-2.5">
            {#if running}
              {#if running.state === "running"}
                <button class="btn btn-lg min-w-[150px] bg-emerald-500/15 text-emerald-300 ring-1 ring-emerald-400/30" disabled={!running.pid} onclick={() => stop(g.id)} title={running.pid ? "Ask the game to quit" : ""}>
                  <span class="live-dot h-2 w-2 rounded-full bg-emerald-400"></span> Playing{#if running.pid}<span class="ml-1 flex items-center gap-1 text-white/70"><Square size={11} fill="currentColor" /> Stop</span>{/if}
                </button>
              {:else}
                <button class="btn btn-lg btn-primary min-w-[150px]" disabled><LoaderCircle size={16} class="spin" /> Launching…</button>
              {/if}
            {:else}
              <button class="btn btn-lg btn-primary min-w-[150px]" disabled={!g.installed} onclick={() => launch(g.id, g.name)} title={g.installed ? "" : "Install this game in its platform first"}>
                <Play size={16} fill="currentColor" /> Play
              </button>
            {/if}
            <button class="btn btn-lg btn-icon w-[46px] {g.favorite ? 'text-accent-ink' : ''}" onclick={toggleFavorite} title={g.favorite ? "Remove from favorites" : "Add to favorites"} aria-label="Toggle favorite">
              <Heart size={17} fill={g.favorite ? "currentColor" : "none"} />
            </button>
            <button class="btn btn-lg btn-icon w-[46px] {tab === 'profile' ? 'text-accent-ink' : ''}" onclick={() => (tab = "profile")} title="Launch settings" aria-label="Launch settings"><Settings2 size={17} /></button>
            {#if g.installDir}
              <button class="btn btn-lg btn-icon w-[46px]" onclick={() => g.installDir && api.openFolder(g.installDir).catch((e) => toastError("Couldn't open folder", e))} title="Open install folder" aria-label="Open install folder"><FolderOpen size={17} /></button>
            {/if}
            <button class="btn btn-lg btn-icon btn-danger w-[46px]" onclick={remove} title="Remove from library" aria-label="Remove from library"><Trash2 size={17} /></button>
          </div>
        </div>
      </div>

      <!-- Stats strip -->
      <div class="rise mt-9 grid grid-cols-2 gap-px overflow-hidden rounded-xl bg-line hairline md:grid-cols-5" style="animation-delay:90ms">
        <div class="bg-surface-1 px-5 py-4" title={g.platformPlaytimeMin ? `${PLATFORM_LABEL[g.platform]} reports ${formatDuration(g.platformPlaytimeMin * 60)} · VANTA tracked ${formatDuration(g.trackedSeconds)}` : ""}>
          <div class="eyebrow">Playtime</div>
          <div class="font-display mt-1.5 text-lg font-semibold">{playtime > 0 ? formatDuration(playtime) : "—"}</div>
          <div class="text-[11px] text-faint">{g.platformPlaytimeMin > 0 ? `reported by ${PLATFORM_LABEL[g.platform]}` : g.trackedSeconds > 0 ? "tracked by VANTA" : "no sessions yet"}</div>
        </div>
        <div class="bg-surface-1 px-5 py-4">
          <div class="eyebrow">Last played</div>
          <div class="font-display mt-1.5 text-lg font-semibold">{relativeTime(g.lastPlayed)}</div>
          <div class="text-[11px] text-faint">{g.lastPlayed ? formatDateTime(g.lastPlayed) : "—"}</div>
        </div>
        <div class="bg-surface-1 px-5 py-4">
          <div class="eyebrow">Sessions</div>
          <div class="font-display mt-1.5 text-lg font-semibold tabular-nums">{g.sessionCount}</div>
          <div class="text-[11px] text-faint">{formatDuration(g.trackedSeconds)} via VANTA</div>
        </div>
        <div class="bg-surface-1 px-5 py-4">
          <div class="eyebrow">Size on disk</div>
          <div class="font-display mt-1.5 text-lg font-semibold">{g.sizeBytes ? formatBytes(g.sizeBytes) : "—"}</div>
          <div class="text-[11px] text-faint">{g.installed ? "installed" : "not installed"}</div>
        </div>
        <div class="bg-surface-1 px-5 py-4">
          <div class="eyebrow">Added</div>
          <div class="font-display mt-1.5 text-lg font-semibold">{relativeTime(g.addedAt)}</div>
          <div class="text-[11px] text-faint">{g.source === "manual" ? "manual import" : "auto-discovered"}</div>
        </div>
      </div>

      <!-- Tabs -->
      <div class="mt-9 flex gap-6 border-b border-line">
        {#each [["overview", "Overview"], ["profile", "Launch profile"], ["sessions", `Sessions${sessions.length ? ` · ${sessions.length}` : ""}`]] as [key, label] (key)}
          <button
            class="relative pb-3 text-[13px] font-medium t transition-colors {tab === key ? 'text-text' : 'text-faint hover:text-dim'}"
            onclick={() => (tab = key as typeof tab)}
          >
            {label}
            {#if tab === key}<span class="absolute inset-x-0 -bottom-px h-[2px] rounded bg-accent"></span>{/if}
          </button>
        {/each}
      </div>

      <div class="fade-in pt-7">
        {#if tab === "overview"}
          <div class="grid gap-10 lg:grid-cols-[minmax(0,1fr)_380px]">
            <div class="space-y-8">
              <section>
                <div class="mb-2 flex items-center justify-between">
                  <h3 class="eyebrow">About</h3>
                  {#if !editingDesc}<button class="text-xs text-faint hover:text-text" onclick={() => ((desc = g.description ?? ""), (editingDesc = true))}>Edit</button>{/if}
                </div>
                {#if editingDesc}
                  <textarea class="field min-h-[120px] leading-relaxed" bind:value={desc} placeholder="Notes, a description, where you left off…"></textarea>
                  <div class="mt-2 flex gap-2">
                    <button class="btn btn-primary h-8" onclick={saveDesc}>Save</button>
                    <button class="btn btn-ghost h-8" onclick={() => (editingDesc = false)}>Cancel</button>
                  </div>
                {:else if g.description}
                  <p class="whitespace-pre-line text-[13.5px] leading-relaxed text-dim">{g.description}</p>
                {:else}
                  <p class="text-[13px] text-faint">No description yet. Add your own notes about this game.</p>
                {/if}
              </section>

              <section>
                <h3 class="eyebrow mb-3">Tags</h3>
                <div class="flex flex-wrap items-center gap-2">
                  {#each g.tags as t (t)}
                    <span class="group inline-flex h-7 items-center gap-1 rounded-full bg-surface-2 pl-3 pr-1.5 text-xs text-dim hairline">
                      #{t}
                      <button class="rounded-full p-0.5 text-faint hover:bg-white/10 hover:text-text" onclick={() => setTags(g.tags.filter((x) => x !== t))} aria-label="Remove tag {t}"><X size={11} /></button>
                    </span>
                  {/each}
                  <input
                    class="h-7 w-36 rounded-full border border-dashed border-line-strong bg-transparent px-3 text-xs outline-none placeholder:text-faint focus:border-accent/60"
                    placeholder="+ Add tag"
                    maxlength="40"
                    bind:value={tagInput}
                    onkeydown={(e) => e.key === "Enter" && addTag()}
                    onblur={addTag}
                  />
                </div>
              </section>
            </div>

            <aside class="space-y-4 text-[12.5px]">
              <h3 class="eyebrow">Details</h3>
              <dl class="divide-y divide-line rounded-xl border border-line bg-surface-1">
                {#each [
                  ["Platform", PLATFORM_LABEL[g.platform]],
                  [g.platform === "steam" ? "AppID" : "Identifier", g.externalId],
                  ["Install folder", g.installDir],
                  ["Executable", g.executable ?? (g.platform === "steam" ? "Managed by Steam" : g.platform === "epic" ? "Managed by Legendary" : null)],
                ] as [k, v] (k)}
                  <div class="flex gap-4 px-4 py-3">
                    <dt class="w-24 shrink-0 text-faint">{k}</dt>
                    <dd class="mono min-w-0 flex-1 break-all text-dim">{v ?? "—"}</dd>
                  </div>
                {/each}
              </dl>
            </aside>
          </div>
        {:else if tab === "profile"}
          <ProfileEditor game={g} />
        {:else}
          {#if sessions.length === 0}
            <div class="py-12 text-center text-[13px] text-faint">
              <Clock size={20} class="mx-auto mb-3" strokeWidth={1.6} />
              No sessions recorded yet. Launch the game from VANTA to start tracking playtime.
            </div>
          {:else}
            <div class="max-w-3xl divide-y divide-line rounded-xl border border-line bg-surface-1">
              {#each sessions as s (s.id)}
                <div class="flex items-center gap-4 px-4 py-3 text-[12.5px]">
                  <span class="h-2 w-2 shrink-0 rounded-full {s.status === 'completed' ? 'bg-emerald-400' : s.status === 'failed' ? 'bg-red-400' : 'bg-amber-300'}"></span>
                  <span class="w-40 text-dim">{formatDateTime(s.startedAt)}</span>
                  <span class="w-24 tabular-nums">{s.status === "completed" ? formatDuration(s.durationS) : "—"}</span>
                  <span class="w-24 text-faint">{statusLabel[s.status]}</span>
                  <span class="min-w-0 flex-1 truncate text-faint" title={s.error ?? ""}>{s.error ?? (s.exitCode != null ? `exit code ${s.exitCode}` : "")}</span>
                </div>
              {/each}
            </div>
          {/if}
        {/if}
      </div>
    </div>
  </div>
{/if}
