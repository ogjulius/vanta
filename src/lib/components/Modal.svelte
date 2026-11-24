<script lang="ts">
  import { X } from "@lucide/svelte";
  import type { Snippet } from "svelte";

  interface Props {
    open: boolean;
    title: string;
    subtitle?: string;
    width?: string;
    onclose: () => void;
    children: Snippet;
    footer?: Snippet;
  }
  let { open, title, subtitle, width = "560px", onclose, children, footer }: Props = $props();

  function onkey(e: KeyboardEvent) {
    if (open && e.key === "Escape") onclose();
  }
</script>

<svelte:window onkeydown={onkey} />

{#if open}
  <div class="fixed inset-0 z-50 flex items-center justify-center p-6">
    <button class="fade-in absolute inset-0 bg-black/70 backdrop-blur-sm" aria-label="Close" onclick={onclose}></button>
    <div
      class="rise relative flex max-h-[88vh] w-full flex-col overflow-hidden rounded-2xl border border-line-strong bg-elevated shadow-2xl shadow-black/60"
      style="max-width:{width}"
      role="dialog"
      aria-modal="true"
      aria-label={title}
    >
      <div class="flex items-start justify-between gap-4 px-6 pb-4 pt-5">
        <div>
          <h2 class="font-display text-lg font-semibold">{title}</h2>
          {#if subtitle}<p class="mt-1 text-[13px] text-dim">{subtitle}</p>{/if}
        </div>
        <button class="btn btn-ghost btn-icon -mr-2 h-8 w-8" onclick={onclose} aria-label="Close"><X size={16} /></button>
      </div>
      <div class="min-h-0 flex-1 overflow-y-auto px-6 pb-5">
        {@render children()}
      </div>
      {#if footer}
        <div class="flex items-center justify-end gap-2 border-t border-line bg-black/20 px-6 py-3.5">
          {@render footer()}
        </div>
      {/if}
    </div>
  </div>
{/if}
