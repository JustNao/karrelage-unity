<script lang="ts">
  import { bindingOf } from "../shortcuts";
  import { shortcuts } from "../stores/shortcuts.svelte";
  import type { Binding } from "../stores/settings.svelte";

  /** A shortcut's binding: click, then press the combination (Échap cancels, Suppr unbinds). */
  interface Props {
    value: Binding;
    onchange: (b: Binding) => void;
    label?: string;
  }
  let { value, onchange, label = "Raccourci" }: Props = $props();
  let listening = $state(false);

  function start() {
    listening = true;
    shortcuts.capturing = true;
  }
  function stop() {
    listening = false;
    shortcuts.capturing = false;
  }
  function key(e: KeyboardEvent) {
    if (!listening) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") return stop();
    if (e.key === "Delete" || e.key === "Backspace") return (onchange({ mods: 0, vk: 0, label: "" }), stop());
    const b = bindingOf(e);
    if (b) (onchange(b), stop());
  }
</script>

<button class="key" class:listening aria-label={label} title="Cliquer puis appuyer sur la combinaison (Échap : annuler, Suppr : aucun raccourci)" onclick={() => (listening ? stop() : start())} onkeydown={key} onblur={stop}>
  {listening ? "Appuyez sur une touche…" : value.vk ? value.label : "Aucun"}
</button>

<style>
  .key { min-width: 150px; font-family: var(--font); }
  .listening { border-color: var(--acc); color: var(--acc); }
</style>
