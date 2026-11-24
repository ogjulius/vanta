<script lang="ts" generics="T extends string">
  let { value, options, onchange, size = "md" }: { value: T; options: { value: T; label: string }[]; onchange: (v: T) => void; size?: "sm" | "md" } = $props();
</script>

<div class="inline-flex rounded-lg bg-surface-2 p-[3px] hairline">
  {#each options as o (o.value)}
    <button
      class="rounded-md font-medium t transition-colors {size === 'sm' ? 'px-2.5 py-1 text-[11.5px]' : 'px-3 py-1.5 text-xs'} {value === o.value
        ? 'bg-surface-3 text-text shadow-sm shadow-black/40'
        : 'text-faint hover:text-dim'}"
      onclick={() => onchange(o.value)}>{o.label}</button
    >
  {/each}
</div>
