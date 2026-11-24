<script lang="ts">
  import { PLATFORM_HUE, PLATFORM_LABEL } from "../format";
  import type { PlatformId } from "../types";

  let { platform, size = "sm" }: { platform: PlatformId; size?: "sm" | "md" } = $props();
</script>

<span
  class="inline-flex items-center gap-1.5 font-medium text-dim {size === 'sm' ? 'text-[11px]' : 'text-xs'}"
  title={PLATFORM_LABEL[platform]}
>
  <span class="inline-block rounded-full {size === 'sm' ? 'h-1.5 w-1.5' : 'h-2 w-2'}" style="background:{PLATFORM_HUE[platform]}"></span>
  {PLATFORM_LABEL[platform]}
</span>
