<script lang="ts">
  let { checked, onchange, disabled = false, label }: { checked: boolean; onchange: (v: boolean) => void; disabled?: boolean; label?: string } = $props();
</script>

<button
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  class="relative inline-flex h-[22px] w-[38px] shrink-0 items-center rounded-full t transition-colors disabled:opacity-40 {checked ? 'bg-accent' : 'bg-surface-3'}"
  onclick={() => onchange(!checked)}
>
  <span
    class="inline-block h-4 w-4 rounded-full bg-white shadow t transition-transform {checked ? 'translate-x-[19px]' : 'translate-x-[3px]'}"
  ></span>
</button>
