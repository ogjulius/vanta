<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { FolderOpen, ImagePlus, Monitor, Plus, Search, LoaderCircle, Check } from "@lucide/svelte";
  import Modal from "./Modal.svelte";
  import Segmented from "./Segmented.svelte";
  import { api, errMsg } from "../api";
  import { app, navigate, toast } from "../state.svelte";
  import type { DesktopGame, ManualGameInput } from "../types";

  let tab = $state<"manual" | "desktop">("manual");
  let form = $state<ManualGameInput>({ name: "", executable: "", launchArgs: "", runner: "native", platform: "local", cover: null });
  let saving = $state(false);
  let error = $state<string | null>(null);

  let entries = $state<DesktopGame[] | null>(null);
  let loadingEntries = $state(false);
  let imported = $state<Set<string>>(new Set());
  let filter = $state("");

  function reset() {
    form = { name: "", executable: "", launchArgs: "", runner: "native", platform: "local", cover: null };
    error = null;
    imported = new Set();
    nameTouched = false;
  }

  function close() {
    app.addGameOpen = false;
    reset();
  }

  async function pickExe() {
    const p = await openDialog({ multiple: false, directory: false, title: "Choose the game executable" });
    if (typeof p === "string") {
      form.executable = p;
      const file = p.split("/").pop() ?? "";
      if (/\.(exe|msi|bat)$/i.test(file)) form.runner = "wine";
      if (!nameTouched) {
        form.name = "";
        const base = file.replace(/\.(exe|sh|x86_64|x86|AppImage|bin)$/i, "").replace(/[_-]+/g, " ").trim();
        form.name = base.charAt(0).toUpperCase() + base.slice(1);
      }
    }
  }

  async function pickCover() {
    const p = await openDialog({ multiple: false, title: "Choose cover art", filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp"] }] });
    if (typeof p === "string") form.cover = p;
  }

  async function save() {
    error = null;
    saving = true;
    try {
      const g = await api.addManualGame({ ...form, name: form.name.trim(), executable: form.executable.trim() });
      toast("success", "Game added", g.name);
      close();
      navigate("game", g.id);
    } catch (e) {
      error = errMsg(e);
    } finally {
      saving = false;
    }
  }

  async function loadEntries() {
    loadingEntries = true;
    try {
      entries = await api.desktopEntries();
    } catch (e) {
      error = errMsg(e);
    } finally {
      loadingEntries = false;
    }
  }

  async function importEntry(d: DesktopGame) {
    try {
      const quoted = d.args.map((a) => (/[\s"'\\]/.test(a) ? `'${a.replace(/'/g, "'\\''")}'` : a)).join(" ");
      await api.addManualGame({ name: d.name, executable: d.executable, launchArgs: quoted, runner: "native", platform: "local" });
      imported = new Set([...imported, d.desktopFile]);
      toast("success", "Imported", d.name);
    } catch (e) {
      toast("error", `Couldn't import ${d.name}`, errMsg(e));
    }
  }

  $effect(() => {
    if (app.addGameOpen && tab === "desktop" && entries === null && !loadingEntries) loadEntries();
  });

  // Derive a default name from the executable when typed or pasted.
  let nameTouched = false;
  function nameFromPath() {
    if (nameTouched || !form.executable.includes("/")) return;
    const file = form.executable.split("/").pop() ?? "";
    if (/\.(exe|msi|bat)$/i.test(file)) form.runner = "wine";
    const parts = form.executable.split("/").filter(Boolean);
    const base = /^(run|start|launch|game)\.(sh|x86_64)$/i.test(file) && parts.length > 1 ? parts[parts.length - 2] : file.replace(/\.(exe|sh|x86_64|x86|AppImage|bin)$/i, "");
    const clean = base.replace(/[_-]+/g, " ").trim();
    form.name = clean.charAt(0).toUpperCase() + clean.slice(1);
  }

  const visibleEntries = $derived((entries ?? []).filter((e) => e.name.toLowerCase().includes(filter.toLowerCase())));
</script>

<Modal open={app.addGameOpen} title="Add a game" subtitle="Import any Linux game, script or Windows executable." width="620px" onclose={close}>
  <div class="mb-5">
    <Segmented
      value={tab}
      options={[
        { value: "manual", label: "Choose executable" },
        { value: "desktop", label: "From installed apps" },
      ]}
      onchange={(v) => (tab = v)}
    />
  </div>

  {#if tab === "manual"}
    <div class="grid grid-cols-[120px_1fr] gap-5">
      <button
        class="group relative flex aspect-[2/3] flex-col items-center justify-center gap-2 overflow-hidden rounded-lg border border-dashed border-line-strong bg-surface-1 text-faint t transition-colors hover:border-accent/50 hover:text-dim"
        onclick={pickCover}
      >
        {#if form.cover}
          <span class="px-2 text-center text-[11px] break-all text-dim">{form.cover.split("/").pop()}</span>
          <span class="text-[10px] text-accent-ink">Change</span>
        {:else}
          <ImagePlus size={20} strokeWidth={1.6} />
          <span class="text-[11px]">Cover art<br /><span class="text-faint">optional</span></span>
        {/if}
      </button>
      <div class="space-y-4">
        <label class="block">
          <span class="eyebrow mb-1.5 block">Executable</span>
          <div class="flex gap-2">
            <input class="field mono" placeholder="/path/to/game" bind:value={form.executable} {@attach (el) => el.focus()} oninput={nameFromPath} />
            <button class="btn shrink-0" onclick={pickExe}><FolderOpen size={15} /> Browse</button>
          </div>
        </label>
        <label class="block">
          <span class="eyebrow mb-1.5 block">Name</span>
          <input class="field" placeholder="Game title" bind:value={form.name} maxlength="256" oninput={() => (nameTouched = form.name.length > 0)} />
        </label>
        <div class="grid grid-cols-2 gap-3">
          <label class="block">
            <span class="eyebrow mb-1.5 block">Runs with</span>
            <select class="field" bind:value={form.runner}>
              <option value="native">Native Linux</option>
              <option value="wine">Wine</option>
              <option value="proton">Proton</option>
            </select>
          </label>
          <label class="block">
            <span class="eyebrow mb-1.5 block">Library</span>
            <select class="field" bind:value={form.platform}>
              <option value="local">Local</option>
              <option value="itch">itch.io</option>
              <option value="gog">GOG</option>
            </select>
          </label>
        </div>
        <label class="block">
          <span class="eyebrow mb-1.5 block">Launch arguments</span>
          <input class="field mono" placeholder="e.g. --fullscreen" bind:value={form.launchArgs} />
        </label>
      </div>
    </div>
    {#if error}<div class="mt-4 rounded-lg border border-red-500/25 bg-red-500/10 px-3 py-2 text-xs text-red-300">{error}</div>{/if}
  {:else}
    <div class="relative mb-3">
      <Search size={14} class="absolute left-3 top-1/2 -translate-y-1/2 text-faint" />
      <input class="field pl-9" placeholder="Filter applications" bind:value={filter} />
    </div>
    {#if loadingEntries}
      <div class="flex items-center justify-center gap-2 py-16 text-sm text-dim"><LoaderCircle size={16} class="spin" /> Scanning desktop entries…</div>
    {:else if visibleEntries.length === 0}
      <div class="py-14 text-center text-[13px] text-dim">
        <Monitor size={22} class="mx-auto mb-3 text-faint" strokeWidth={1.6} />
        No importable games found in your desktop entries.<br />
        <span class="text-faint">Apps in the “Game” category appear here. Store games are imported by their integrations.</span>
      </div>
    {:else}
      <div class="space-y-1">
        {#each visibleEntries as d (d.desktopFile)}
          {@const done = imported.has(d.desktopFile)}
          <div class="flex items-center gap-3 rounded-lg px-3 py-2.5 hover:bg-white/[0.03]">
            <div class="min-w-0 flex-1">
              <div class="truncate text-[13px] font-medium">{d.name}</div>
              <div class="mono truncate text-faint">{d.executable} {d.args.join(" ")}</div>
            </div>
            <button class="btn h-8 shrink-0 {done ? 'text-emerald-400' : ''}" disabled={done} onclick={() => importEntry(d)}>
              {#if done}<Check size={14} /> Added{:else}<Plus size={14} /> Import{/if}
            </button>
          </div>
        {/each}
      </div>
    {/if}
  {/if}

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={close}>{tab === "desktop" ? "Done" : "Cancel"}</button>
    {#if tab === "manual"}
      <button class="btn btn-primary" disabled={saving || !form.name.trim() || !form.executable.trim()} onclick={save}>
        {#if saving}<LoaderCircle size={15} class="spin" />{:else}<Plus size={15} />{/if} Add to library
      </button>
    {/if}
  {/snippet}
</Modal>
