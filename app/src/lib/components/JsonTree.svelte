<script lang="ts">
  import JsonTree from "./JsonTree.svelte";

  /** Decoded message tree. Keys carrying `__t`/`__f` meta are editable: click to name a field. */
  let { v, onrename }: { v: any; onrename?: (msg: string, field: string, name: string) => void } = $props();

  let editing = $state<string | null>(null);
  let draft = $state("");

  const CAND = / \((.*)\?\)$/;
  const keys = $derived(v && typeof v === "object" && !Array.isArray(v) ? Object.keys(v).filter((k) => !k.startsWith("__")) : []);
  const scalarList = $derived(Array.isArray(v) && v.every((x) => typeof x !== "object" || x === null));
  const meta = $derived<Record<string, string>>(v?.__f ?? {});
  const msg = $derived<string | undefined>(v?.__t);

  const cands = (label: string) => label.match(CAND)?.[1].split("|") ?? [];
  const manual = (label: string) => !CAND.test(label) && label !== meta[label];

  function start(label: string) {
    editing = label;
    draft = CAND.test(label) || label === meta[label] ? "" : label;
  }
  function commit(label: string) {
    if (editing === null) return;
    const name = draft.trim();
    editing = null;
    if (msg && onrename) onrename(msg, meta[label], name);
  }
</script>

{#if v === null}
  <span class="jn">null</span>
{:else if Array.isArray(v)}
  {#if !v.length}[]
  {:else if scalarList}[{#each v as x, i}<JsonTree v={x} />{i < v.length - 1 ? ", " : ""}{/each}]
  {:else}[<div class="ind">{#each v as x}<div><JsonTree v={x} {onrename} />,</div>{/each}</div>]{/if}
{:else if typeof v === "object"}
  {#if !keys.length}{"{}"}
  {:else}{"{"}<div class="ind">
      {#each keys as k (k)}
        <div>
          {#if editing === k}
            {@const dl = "dl-" + k.replace(/\W/g, "_")}
            <!-- svelte-ignore a11y_autofocus -->
            <input
              class="ren"
              list={dl}
              bind:value={draft}
              placeholder={meta[k]}
              autofocus
              onkeydown={(e) => (e.key === "Enter" ? commit(k) : e.key === "Escape" && (editing = null))}
              onblur={() => (editing = null)}
            />
            <datalist id={dl}>{#each cands(k) as c}<option value={c}></option>{/each}</datalist>
          {:else if msg && meta[k]}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <span class="jk ed" class:man={manual(k)} title="click to name this field ({meta[k]})" onclick={() => start(k)}>{k}</span>
          {:else}
            <span class="jk">{k}</span>
          {/if}: <JsonTree v={v[k]} {onrename} />
        </div>
      {/each}
    </div>{"}"}
  {/if}
{:else if typeof v === "string"}
  <span class="js">"{v}"</span>
{:else}
  <span class="jn">{v}</span>
{/if}

<style>
  .ind { padding-left: 16px; }
  .jk { color: var(--acc); }
  .jk.ed { cursor: pointer; border-bottom: 1px dotted var(--mut); }
  .jk.ed:hover { background: color-mix(in srgb, var(--acc) 20%, transparent); }
  .jk.man { color: var(--ok); }
  .js { color: var(--ok); }
  .jn { color: var(--warn); }
  .ren { width: 240px; }
</style>
