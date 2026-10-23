<script lang="ts" generics="T extends string">
  /** One choice among a few, as joined buttons (radio group): inner tabs, display levels. `bind:value`. */
  let {
    value = $bindable(),
    options,
    label,
    disabled = false,
  }: { value: T; options: { id: T; label: string; title?: string }[]; label: string; disabled?: boolean } = $props();
</script>

<div class="seg" role="radiogroup" aria-label={label} class:off={disabled}>
  {#each options as o (o.id)}
    <button role="radio" aria-checked={value === o.id} class:on={value === o.id} title={o.title} onclick={() => (value = o.id)}>{o.label}</button>
  {/each}
</div>

<style>
  .seg { display: inline-flex; border: 1px solid var(--line); border-radius: 6px; overflow: hidden; flex: none; }
  .seg button { border: 0; border-radius: 0; background: none; color: var(--mut); padding: 4px 10px; font-size: 12px; }
  .seg button + button { border-left: 1px solid var(--line); }
  .seg button.on { background: var(--panel); color: var(--fg); font-weight: 600; }
  .seg.off { opacity: 0.5; }
</style>
