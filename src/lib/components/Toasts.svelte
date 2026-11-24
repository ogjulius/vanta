<script lang="ts">
  import { CircleCheck, CircleAlert, Info, TriangleAlert, X } from "@lucide/svelte";
  import { dismissToast, toasts } from "../state.svelte";

  const icon = { success: CircleCheck, error: CircleAlert, warning: TriangleAlert, info: Info };
  const tone = { success: "text-emerald-400", error: "text-red-400", warning: "text-amber-300", info: "text-accent-ink" };
</script>

<div class="pointer-events-none fixed bottom-5 right-5 z-[60] flex w-[380px] flex-col gap-2">
  {#each toasts as t (t.id)}
    {@const Icon = icon[t.kind]}
    <div class="rise glass pointer-events-auto flex gap-3 rounded-xl border border-line-strong p-3.5 shadow-2xl shadow-black/50" role="status">
      <Icon size={18} class="mt-px shrink-0 {tone[t.kind]}" />
      <div class="min-w-0 flex-1">
        <div class="text-[13px] font-medium">{t.title}</div>
        {#if t.message}<div class="mt-0.5 max-h-40 overflow-y-auto whitespace-pre-wrap break-words text-xs leading-relaxed text-dim">{t.message}</div>{/if}
      </div>
      <button class="-m-1 h-6 w-6 shrink-0 rounded text-faint hover:text-text" onclick={() => dismissToast(t.id)} aria-label="Dismiss">
        <X size={14} class="mx-auto" />
      </button>
    </div>
  {/each}
</div>
