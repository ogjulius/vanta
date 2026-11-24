<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { FolderOpen, Plus, RotateCcw, Save, Trash2, LoaderCircle, Terminal } from "@lucide/svelte";
  import { api, errMsg } from "../api";
  import { confirmAction, toast } from "../state.svelte";
  import type { Game, GameProfile, Runner } from "../types";

  let { game }: { game: Game } = $props();

  let profile = $state<GameProfile | null>(null);
  let saved = $state("");
  let runners = $state<Runner[]>([]);
  let preview = $state<string[] | null>(null);
  let previewError = $state<string | null>(null);
  let saving = $state(false);
  let error = $state<string | null>(null);

  const isSteam = $derived(game.platform === "steam");
  const viaLauncher = $derived(game.platform === "gog" && !game.executable);
  const supportsRuntime = $derived(!isSteam && !viaLauncher);
  const dirty = $derived(profile ? JSON.stringify(profile) !== saved : false);

  async function load() {
    try {
      const p = await api.getProfile(game.id);
      profile = p;
      saved = JSON.stringify(p);
      refreshPreview();
    } catch (e) {
      error = errMsg(e);
    }
  }

  async function refreshPreview() {
    try {
      preview = await api.previewLaunch(game.id);
      previewError = null;
    } catch (e) {
      preview = null;
      previewError = errMsg(e);
    }
  }

  $effect(() => {
    void game.id;
    load();
    api.detectRunners().then((r) => (runners = r)).catch(() => {});
  });

  async function save() {
    if (!profile) return;
    saving = true;
    error = null;
    try {
      const clean = { ...profile, env: profile.env.filter((e) => e.key.trim() || e.value.trim()) };
      profile = await api.saveProfile(clean);
      saved = JSON.stringify(profile);
      toast("success", "Launch profile saved");
      refreshPreview();
    } catch (e) {
      error = errMsg(e);
    } finally {
      saving = false;
    }
  }

  async function reset() {
    const ok = await confirmAction({ title: "Reset launch profile?", message: `All custom launch settings for ${game.name} will be cleared.`, confirmLabel: "Reset", danger: true });
    if (!ok) return;
    try {
      profile = await api.resetProfile(game.id);
      saved = JSON.stringify(profile);
      toast("success", "Profile reset to defaults");
      refreshPreview();
    } catch (e) {
      error = errMsg(e);
    }
  }

  async function pick(field: "workingDir" | "executableOverride" | "winePrefix" | "wineBinary" | "protonPath", directory: boolean) {
    if (!profile) return;
    const p = await openDialog({ multiple: false, directory, defaultPath: game.installDir ?? undefined });
    if (typeof p === "string") profile[field] = p;
  }
</script>

{#if profile}
  <div class="grid max-w-3xl gap-6">
    <div>
      <label class="eyebrow mb-1.5 block" for="args">Launch arguments</label>
      <input id="args" class="field mono" placeholder={isSteam ? "e.g. -novid -fullscreen" : "e.g. --fullscreen --skip-intro"} bind:value={profile.launchArgs} />
      <p class="mt-1.5 text-[11.5px] text-faint">
        Quotes group words. Arguments are passed directly to the game — never through a shell.
        {#if isSteam}Steam games receive them via <span class="mono">steam -applaunch</span>.{/if}
      </p>
    </div>

    {#if isSteam}
      <div class="rounded-lg border border-line bg-surface-1 px-4 py-3 text-[12.5px] leading-relaxed text-dim">
        Steam launches this game itself, so environment variables, wrappers and the Proton version are configured in Steam
        (<span class="text-text">Properties → General → Launch options</span> and <span class="text-text">Compatibility</span>).
      </div>
    {:else if viaLauncher}
      <div class="rounded-lg border border-line bg-surface-1 px-4 py-3 text-[12.5px] leading-relaxed text-dim">
        This game is launched through Heroic, which owns its Wine configuration. Configure runtime options in Heroic.
      </div>
    {/if}

    {#if supportsRuntime}
      <div class="grid grid-cols-2 gap-4">
        <div>
          <label class="eyebrow mb-1.5 block" for="runner">Runner</label>
          <select id="runner" class="field" bind:value={profile.runner}>
            <option value="default">Automatic</option>
            <option value="native">Native Linux</option>
            <option value="wine">Wine</option>
            <option value="proton">Proton</option>
          </select>
        </div>
        <div>
          <label class="eyebrow mb-1.5 block" for="wrapper">Wrapper command</label>
          <input id="wrapper" class="field mono" placeholder="e.g. gamemoderun mangohud" bind:value={profile.wrapper} />
        </div>
      </div>

      {#if profile.runner === "wine" || profile.runner === "proton" || game.platform === "epic"}
        <div class="grid gap-4 rounded-xl border border-line bg-surface-1 p-4">
          {#if profile.runner === "proton" && game.platform !== "epic"}
            <div>
              <span class="eyebrow mb-1.5 block">Proton version</span>
              <div class="flex gap-2">
                <select class="field" value={profile.protonPath ?? ""} onchange={(e) => profile && (profile.protonPath = e.currentTarget.value || null)}>
                  <option value="">Auto (first detected)</option>
                  {#each runners.filter((r) => r.kind === "proton") as r (r.path)}<option value={r.path}>{r.name}</option>{/each}
                  {#if profile.protonPath && !runners.some((r) => r.path === profile?.protonPath)}<option value={profile.protonPath}>{profile.protonPath}</option>{/if}
                </select>
                <button class="btn shrink-0" onclick={() => pick("protonPath", false)}><FolderOpen size={14} /></button>
              </div>
            </div>
          {:else}
            <div>
              <span class="eyebrow mb-1.5 block">Wine executable</span>
              <div class="flex gap-2">
                <select class="field" value={profile.wineBinary ?? ""} onchange={(e) => profile && (profile.wineBinary = e.currentTarget.value || null)}>
                  <option value="">{game.platform === "epic" ? "Legendary default" : "System wine"}</option>
                  {#each runners.filter((r) => r.kind === "wine") as r (r.path)}<option value={r.path}>{r.name}</option>{/each}
                  {#if profile.wineBinary && !runners.some((r) => r.path === profile?.wineBinary)}<option value={profile.wineBinary}>{profile.wineBinary}</option>{/if}
                </select>
                <button class="btn shrink-0" onclick={() => pick("wineBinary", false)}><FolderOpen size={14} /></button>
              </div>
              {#if !runners.some((r) => r.kind === "wine")}<p class="mt-1.5 text-[11.5px] text-amber-300/80">No Wine installation detected. Install it with <span class="mono">sudo pacman -S wine</span>.</p>{/if}
            </div>
          {/if}
          <div>
            <span class="eyebrow mb-1.5 block">Prefix</span>
            <div class="flex gap-2">
              <input class="field mono" placeholder="Default: a private prefix managed by VANTA" bind:value={profile.winePrefix} />
              <button class="btn shrink-0" onclick={() => pick("winePrefix", true)}><FolderOpen size={14} /></button>
            </div>
          </div>
        </div>
      {/if}

      <div class="grid grid-cols-2 gap-4">
        <div>
          <span class="eyebrow mb-1.5 block">Working directory</span>
          <div class="flex gap-2">
            <input class="field mono" placeholder={game.installDir ?? "Executable's folder"} bind:value={profile.workingDir} />
            <button class="btn shrink-0" onclick={() => pick("workingDir", true)}><FolderOpen size={14} /></button>
          </div>
        </div>
        {#if game.platform !== "epic"}
          <div>
            <span class="eyebrow mb-1.5 block">Executable override</span>
            <div class="flex gap-2">
              <input class="field mono" placeholder={game.executable ?? "Use the detected executable"} bind:value={profile.executableOverride} />
              <button class="btn shrink-0" onclick={() => pick("executableOverride", false)}><FolderOpen size={14} /></button>
            </div>
          </div>
        {/if}
      </div>

      <div>
        <div class="mb-2 flex items-center justify-between">
          <span class="eyebrow">Environment variables</span>
          <button class="btn btn-ghost h-7 px-2 text-xs" onclick={() => profile?.env.push({ key: "", value: "" })}><Plus size={13} /> Add</button>
        </div>
        {#if profile.env.length === 0}
          <p class="text-[12px] text-faint">None. Common examples: <span class="mono">DXVK_HUD=fps</span>, <span class="mono">PROTON_ENABLE_NVAPI=1</span>, <span class="mono">MANGOHUD=1</span>.</p>
        {/if}
        <div class="space-y-2">
          {#each profile.env as env, i (i)}
            <div class="flex gap-2">
              <input class="field mono w-[40%]" placeholder="NAME" bind:value={env.key} />
              <input class="field mono flex-1" placeholder="value" bind:value={env.value} />
              <button class="btn btn-ghost btn-icon btn-danger shrink-0" onclick={() => profile?.env.splice(i, 1)} aria-label="Remove variable"><Trash2 size={14} /></button>
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <div>
      <div class="eyebrow mb-1.5 flex items-center gap-1.5"><Terminal size={12} /> Launch command {dirty ? "(saved version)" : ""}</div>
      {#if preview}
        <pre class="mono overflow-x-auto whitespace-pre-wrap break-all rounded-lg border border-line bg-black/40 px-3.5 py-3 leading-relaxed text-dim">{preview.map((a) => (/\s/.test(a) ? `"${a}"` : a)).join(" ")}</pre>
      {:else if previewError}
        <div class="rounded-lg border border-amber-400/20 bg-amber-400/[0.06] px-3.5 py-3 text-xs text-amber-200/90">{previewError}</div>
      {/if}
    </div>

    {#if error}<div class="rounded-lg border border-red-500/25 bg-red-500/10 px-3 py-2 text-xs text-red-300">{error}</div>{/if}

    <div class="flex items-center gap-2">
      <button class="btn btn-primary" disabled={!dirty || saving} onclick={save}>
        {#if saving}<LoaderCircle size={15} class="spin" />{:else}<Save size={15} />{/if} Save profile
      </button>
      <button class="btn btn-ghost" onclick={reset}><RotateCcw size={14} /> Reset to default</button>
    </div>
  </div>
{:else if error}
  <div class="text-sm text-red-300">{error}</div>
{:else}
  <div class="skeleton h-40 max-w-3xl rounded-xl"></div>
{/if}
