<script lang="ts">
  import type { Component, Snippet } from "svelte";
  let { icon: Icon, title, message, children }: { icon: Component<{ size?: number; class?: string; strokeWidth?: number }>; title: string; message?: string; children?: Snippet } = $props();
</script>

<div class="fade-in flex flex-col items-center justify-center px-6 py-20 text-center">
  <div class="relative mb-5">
    <div class="absolute inset-0 rounded-full bg-accent/20 blur-2xl"></div>
    <div class="relative flex h-14 w-14 items-center justify-center rounded-2xl bg-surface-2 hairline">
      <Icon size={22} strokeWidth={1.6} class="text-accent-ink" />
    </div>
  </div>
  <h3 class="font-display text-[17px] font-semibold">{title}</h3>
  {#if message}<p class="mt-2 max-w-md text-[13px] leading-relaxed text-dim">{message}</p>{/if}
  {#if children}<div class="mt-6 flex gap-2">{@render children()}</div>{/if}
</div>
