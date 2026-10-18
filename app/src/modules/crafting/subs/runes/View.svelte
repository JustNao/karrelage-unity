<script lang="ts">
  import RuneTable from "../../RuneTable.svelte";
  import SmoothScroll from "../../../../lib/components/SmoothScroll.svelte";

  /** "Rune Ra Fo" -> Ra / "Fo"; "Rune Fo" -> Base / "Fo". Special runes ("Rune de chasse", "Rune de Signature"...) are left out. */
  const forge = (n: string) => {
    const m = n.match(/^Rune (?:(Pa|Ra) )?(?!de )(.+)$/);
    return m ? { tier: m[1] ?? "Base", stat: m[2] } : null;
  };
  const transcend = (n: string) => {
    const m = n.match(/^Rune (Ta|Pata|Rata) (.+)$/);
    return m ? { tier: m[1], stat: m[2] } : null;
  };
</script>

<div class="page">
  <SmoothScroll>
    <RuneTable kind="forge" title="Runes de forgemagie" tiers={["Base", "Pa", "Ra"]} parse={forge} />
    <RuneTable kind="transcend" title="Runes de transcendance" tiers={["Ta", "Pata", "Rata"]} parse={transcend} />
  </SmoothScroll>
</div>

<style>
  .page { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .page > :global(.wrap) { flex: 1; }
</style>
