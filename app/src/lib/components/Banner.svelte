<script lang="ts">
  import type { Snippet } from "svelte";

  /** A message strip tinted by its tone (errors, warnings, done). `inset` = rounded with a margin (inside a page), else full width. */
  let { tone = "bad", inset = false, children }: { tone?: "bad" | "warn" | "ok"; inset?: boolean; children: Snippet } = $props();
</script>

<div class="banner tone-{tone}" class:inset>{@render children()}</div>

<style>
  .banner { padding: 6px 12px; font-family: var(--font); }
  .banner.inset { padding: 6px 10px; margin-bottom: 8px; border-radius: 6px; }
  .tone-bad { background: color-mix(in srgb, var(--bad) 20%, transparent); }
  .tone-warn { background: color-mix(in srgb, var(--warn) 20%, transparent); }
  .tone-ok { background: color-mix(in srgb, var(--ok) 20%, transparent); }
</style>
