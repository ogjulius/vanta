<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { CircleAlert, X } from "@lucide/svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import Toasts from "./lib/components/Toasts.svelte";
  import ConfirmDialog from "./lib/components/ConfirmDialog.svelte";
  import AddGameModal from "./lib/components/AddGameModal.svelte";
  import Home from "./lib/views/Home.svelte";
  import Library from "./lib/views/Library.svelte";
  import GameDetail from "./lib/views/GameDetail.svelte";
  import Downloads from "./lib/views/Downloads.svelte";
  import Statistics from "./lib/views/Statistics.svelte";
  import Performance from "./lib/views/Performance.svelte";
  import Settings from "./lib/views/Settings.svelte";
  import { app, back, boot, confirmAction, nav, navigate, type View } from "./lib/state.svelte";

  let scroller = $state<HTMLElement>();

  onMount(() => {
    boot();
    const win = getCurrentWindow();
    let closing = false;
    const unlisten = win.onCloseRequested(async (event) => {
      if (closing || !app.settings.confirmBeforeClose) return;
      event.preventDefault();
      const running = Object.keys(app.running).length;
      const ok = await confirmAction({
        title: "Quit VANTA?",
        message: running ? "Your game will keep running, but VANTA won't record the end of this session." : "Are you sure you want to close VANTA?",
        confirmLabel: "Quit",
      });
      if (ok) {
        closing = true;
        await win.destroy();
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  });

  // Reset scroll on navigation.
  $effect(() => {
    void nav.view;
    void nav.gameId;
    scroller?.scrollTo({ top: 0 });
  });

  function onKey(e: KeyboardEvent) {
    const target = e.target as HTMLElement;
    if (e.key === "Backspace" && nav.view === "game" && !["INPUT", "TEXTAREA"].includes(target.tagName)) back();
    if (e.altKey && e.key === "ArrowLeft") back();
    // Ctrl+1…8 jump to sidebar sections, Ctrl+, opens Settings.
    if (e.ctrlKey && !e.shiftKey && !e.altKey) {
      const order: View[] = ["home", "library", "installed", "favorites", "recent", "downloads", "stats", "performance"];
      const n = Number(e.key);
      if (n >= 1 && n <= order.length) {
        e.preventDefault();
        navigate(order[n - 1]);
      } else if (e.key === ",") {
        e.preventDefault();
        navigate("settings");
      } else if (e.key.toLowerCase() === "n") {
        e.preventDefault();
        app.addGameOpen = true;
      }
    }
  }
</script>

<svelte:window onkeydown={onKey} />

{#if app.bootError}
  <div class="flex h-full flex-col items-center justify-center gap-3 p-10 text-center">
    <CircleAlert size={30} class="text-red-400" />
    <h1 class="font-display text-xl font-semibold">VANTA couldn't start</h1>
    <p class="max-w-lg text-sm text-dim">{app.bootError}</p>
  </div>
{:else if !app.ready}
  <div class="flex h-full items-center justify-center">
    <img src="/vanta.svg" alt="" class="h-14 w-14 animate-pulse opacity-80" />
  </div>
{:else}
  <div class="flex h-full">
    <Sidebar />
    <main bind:this={scroller} class="relative min-w-0 flex-1 overflow-y-auto overflow-x-hidden">
      {#if app.notice}
        <div class="sticky top-0 z-30 flex items-start gap-3 border-b border-amber-400/20 bg-amber-950/80 px-6 py-3 text-[12.5px] text-amber-100 backdrop-blur">
          <CircleAlert size={16} class="mt-px shrink-0" />
          <span class="flex-1">{app.notice}</span>
          <button onclick={() => (app.notice = null)} aria-label="Dismiss"><X size={14} /></button>
        </div>
      {/if}
      {#key nav.view + (nav.gameId ?? "")}
        <div class="fade-in min-h-full">
          {#if nav.view === "home"}
            <Home />
          {:else if nav.view === "library" || nav.view === "installed" || nav.view === "favorites" || nav.view === "recent"}
            <Library mode={nav.view} />
          {:else if nav.view === "game" && nav.gameId}
            <GameDetail id={nav.gameId} />
          {:else if nav.view === "downloads"}
            <Downloads />
          {:else if nav.view === "stats"}
            <Statistics />
          {:else if nav.view === "performance"}
            <Performance />
          {:else if nav.view === "settings"}
            <Settings />
          {/if}
        </div>
      {/key}
    </main>
  </div>
  <AddGameModal />
{/if}
<Toasts />
<ConfirmDialog />
