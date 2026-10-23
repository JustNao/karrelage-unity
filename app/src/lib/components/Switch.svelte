<script lang="ts">
  /** On / off switch. `bind:checked`, or `checked` + `onchange` when the caller applies the change itself (e.g. after a call). */
  let {
    checked = $bindable(false),
    label,
    title = "",
    disabled = false,
    onchange,
  }: { checked?: boolean; label: string; title?: string; disabled?: boolean; onchange?: (on: boolean) => void } = $props();
</script>

<button
  class="sw"
  class:on={checked}
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {title}
  {disabled}
  onclick={() => (onchange ? onchange(!checked) : (checked = !checked))}
></button>

<style>
  .sw { position: relative; width: 44px; height: 24px; padding: 0; background: var(--line); border-radius: 12px; transition: 0.15s; flex: none; border: 0; }
  .sw::after { content: ""; position: absolute; top: 3px; left: 3px; width: 18px; height: 18px; border-radius: 50%; background: #fff; transition: 0.15s; }
  .sw.on { background: var(--ok); }
  .sw.on::after { left: 23px; }
</style>
