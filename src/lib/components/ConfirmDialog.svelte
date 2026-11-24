<script lang="ts">
  import Modal from "./Modal.svelte";
  import { confirmState } from "../state.svelte";

  function close(v: boolean) {
    confirmState.open = false;
    confirmState.resolve?.(v);
    confirmState.resolve = null;
  }
</script>

<Modal open={confirmState.open} title={confirmState.title} width="440px" onclose={() => close(false)}>
  <p class="whitespace-pre-line text-[13px] leading-relaxed text-dim">{confirmState.message}</p>
  {#snippet footer()}
    <button class="btn btn-ghost" onclick={() => close(false)}>Cancel</button>
    <button class="btn {confirmState.danger ? 'bg-red-500/90 text-white hover:!bg-red-500' : 'btn-primary'}" onclick={() => close(true)}>
      {confirmState.confirmLabel}
    </button>
  {/snippet}
</Modal>
