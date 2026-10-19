<script lang="ts">
  import { onDestroy } from "svelte";

  /**
   * A button for destructive actions: the first click turns it into "Confirmer" (in red) for a few seconds, the second
   * click runs `onconfirm`. Leaving the button (blur) or waiting cancels.
   */
  let {
    label,
    confirmLabel = "Confirmer",
    title,
    mini = false,
    disabled = false,
    timeout = 4000,
    onconfirm,
  }: { label: string; confirmLabel?: string; title?: string; mini?: boolean; disabled?: boolean; timeout?: number; onconfirm: () => void | Promise<unknown> } = $props();

  let armed = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const disarm = () => {
    clearTimeout(timer);
    armed = false;
  };
  function click(e: MouseEvent) {
    e.stopPropagation(); // inside clickable rows (e.g. a build of the Stuff list)
    if (!armed) {
      armed = true;
      timer = setTimeout(() => (armed = false), timeout);
      return;
    }
    disarm();
    void onconfirm();
  }
  onDestroy(() => clearTimeout(timer));
</script>

<button class:mini class:danger={armed} {disabled} {title} onclick={click} onblur={disarm}>{armed ? confirmLabel : label}</button>

<style>
  .danger { color: var(--bad); border-color: var(--bad); }
</style>
