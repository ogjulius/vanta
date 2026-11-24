<script lang="ts">
  import { assetUrl } from "../api";
  import { hashHue, PLATFORM_LABEL } from "../format";
  import type { PlatformId } from "../types";

  interface Props {
    name: string;
    path: string | null;
    kind?: "cover" | "hero";
    platform?: PlatformId;
    class?: string;
    eager?: boolean;
  }
  let { name, path, kind = "cover", platform, class: klass = "", eager = false }: Props = $props();

  let failed = $state(false);
  let loaded = $state(false);
  const src = $derived(failed ? null : assetUrl(path));
  const hue = $derived(hashHue(name));

  $effect(() => {
    // Reset when the image source changes.
    void path;
    failed = false;
    loaded = false;
  });
</script>

<div class="art relative overflow-hidden {klass}" style="--h:{hue}">
  {#if src}
    <img
      {src}
      alt=""
      loading={eager ? "eager" : "lazy"}
      decoding="async"
      draggable="false"
      class="absolute inset-0 h-full w-full object-cover t transition-opacity"
      class:opacity-0={!loaded}
      onload={() => (loaded = true)}
      onerror={() => (failed = true)}
    />
  {/if}
  {#if !src || !loaded}
    <!-- Generated fallback: tasteful gradient + typographic title -->
    <div class="fallback absolute inset-0" class:hero={kind === "hero"}>
      <div class="absolute inset-0 noise"></div>
      {#if kind === "cover"}
        <div class="fb-text absolute inset-x-0 bottom-0 p-[9%]">
          {#if platform}
            <div class="fb-eyebrow mb-2 text-[9px] font-semibold uppercase tracking-[0.18em] text-white/40">{PLATFORM_LABEL[platform]}</div>
          {/if}
          <div class="fb-title font-display font-semibold leading-[1.15] text-white/90 line-clamp-4">{name}</div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .fallback {
    background:
      radial-gradient(120% 80% at 85% 0%, hsl(var(--h) 60% 45% / 0.35), transparent 60%),
      radial-gradient(90% 70% at 0% 100%, hsl(calc(var(--h) + 50) 55% 35% / 0.25), transparent 70%),
      linear-gradient(165deg, hsl(var(--h) 22% 13%), hsl(calc(var(--h) + 30) 18% 6%));
  }
  .fallback.hero {
    background:
      radial-gradient(60% 90% at 75% 20%, hsl(var(--h) 55% 40% / 0.4), transparent 70%),
      radial-gradient(50% 80% at 10% 90%, hsl(calc(var(--h) + 60) 50% 30% / 0.3), transparent 70%),
      linear-gradient(180deg, hsl(var(--h) 20% 10%), #050507);
  }
  .art {
    container-type: inline-size;
  }
  .fb-title {
    font-size: clamp(7px, 10.5cqw, 22px);
  }
  @container (max-width: 90px) {
    .fb-eyebrow {
      display: none;
    }
    .fb-title {
      font-size: 0;
    }
    .fb-title::first-letter {
      font-size: 34cqw;
    }
    .fb-text {
      inset: 0;
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 0;
    }
  }
  .noise {
    opacity: 0.18;
    mix-blend-mode: overlay;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='160' height='160'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='.9' numOctaves='2' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E");
  }
</style>
