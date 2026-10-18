<script lang="ts">
  import { tick } from "svelte";
  import { FILTER_GROUPS, norm, type EffectMeta } from "../effects";
  import SmoothScroll from "./SmoothScroll.svelte";

  /**
   * Type-ahead effect picker: a dropdown grouped like the in-game characteristics panel (main stats, secondary stats,
   * damage, resistances) that narrows while typing. The best match is auto-filled as a ghost completion (Tab accepts it,
   * Enter picks the highlighted one).
   */
  let { effects, onpick, placeholder = "Add any effect…" }: { effects: EffectMeta[]; onpick: (id: number) => void; placeholder?: string } = $props();

  let q = $state("");
  let open = $state(false);
  let hi = $state(0);
  let input: HTMLInputElement;
  let box = $state({ top: 0, left: 0, width: 0 });

  const matches = $derived.by(() => {
    const n = norm(q.trim());
    const hit = effects.filter((e) => !n || norm(e.label).includes(n));
    // prefix matches first so the auto-filled suggestion is the most natural completion
    return [...hit].sort((a, b) => Number(!norm(a.label).startsWith(n)) - Number(!norm(b.label).startsWith(n)));
  });
  const sections = $derived(
    FILTER_GROUPS.map((g) => ({ ...g, items: matches.filter((e) => g.members.includes(e.group)) })).filter((s) => s.items.length),
  );
  // keyboard order = visual order (section by section)
  const flat = $derived(sections.flatMap((s) => s.items));
  const best = $derived(flat[Math.min(hi, flat.length - 1)]);
  const ghost = $derived.by(() => {
    const n = norm(q);
    return q && best && norm(best.label).startsWith(n) ? best.label.slice(q.length) : "";
  });

  function place() {
    const r = input.getBoundingClientRect();
    box = { top: r.bottom + 4, left: r.left, width: r.width };
  }
  function show() {
    place();
    open = true;
  }
  function pick(e: EffectMeta) {
    onpick(e.id);
    q = "";
    hi = 0;
    open = false;
  }
  async function move(d: number) {
    if (!open) return show();
    hi = (hi + d + flat.length) % flat.length;
    await tick();
    document.getElementById(`ep-${flat[hi]?.id}`)?.scrollIntoView({ block: "nearest" });
  }
  function key(e: KeyboardEvent) {
    if (e.key === "ArrowDown") (e.preventDefault(), move(1));
    else if (e.key === "ArrowUp") (e.preventDefault(), move(-1));
    else if (e.key === "Enter" && open && best) (e.preventDefault(), pick(best));
    else if (e.key === "Tab" && ghost) (e.preventDefault(), (q = best.label));
    else if (e.key === "Escape") open = false;
  }
</script>

<svelte:window onresize={() => open && place()} />

<div class="picker">
  <div class="ghost" aria-hidden="true"><span class="typed">{q}</span>{ghost}</div>
  <input
    bind:this={input}
    bind:value={q}
    {placeholder}
    autocomplete="off"
    spellcheck="false"
    onfocus={show}
    oninput={() => ((hi = 0), show())}
    onkeydown={key}
    onblur={() => (open = false)}
  />
</div>

{#if open}
  <!-- mousedown is prevented so clicking an item does not blur (and close) the input before the pick -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="drop" style:top="{box.top}px" style:left="{box.left}px" style:width="{box.width}px" onmousedown={(e) => e.preventDefault()}>
    <SmoothScroll pad="4px" maxHeight="300px">
      {#each sections as s (s.key)}
        <div class="head">{s.label}</div>
        {#each s.items as e (e.id)}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div id="ep-{e.id}" class="opt" class:hi={flat[hi]?.id === e.id} onmouseenter={() => (hi = flat.indexOf(e))} onclick={() => pick(e)}>{e.label}</div>
        {/each}
      {:else}
        <div class="none">No matching effect</div>
      {/each}
    </SmoothScroll>
  </div>
{/if}

<style>
  .picker { position: relative; flex: 1; min-width: 0; }
  input { width: 100%; position: relative; background: transparent; }
  .ghost {
    position: absolute;
    inset: 0;
    padding: 5px 9px; /* input padding + border */
    white-space: pre;
    overflow: hidden;
    color: var(--mut);
    opacity: 0.7;
    pointer-events: none;
    font: inherit;
  }
  .ghost .typed { visibility: hidden; }
  .picker::before { content: ""; position: absolute; inset: 0; background: var(--bg); border-radius: 6px; }
  .drop {
    position: fixed;
    z-index: 50;
    background: #07080a;
    border: 1px solid #343944;
    border-radius: 8px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.55);
    overflow: hidden;
    font-family: system-ui, sans-serif;
  }
  .head { padding: 6px 8px 2px; font-size: 10px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--mut); }
  .opt { padding: 4px 8px; border-radius: 5px; cursor: pointer; font-size: 12.5px; }
  .opt.hi { background: color-mix(in srgb, var(--acc) 22%, transparent); }
  .none { padding: 8px; color: var(--mut); font-size: 12px; }
</style>
