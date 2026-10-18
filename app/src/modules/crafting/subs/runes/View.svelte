<script lang="ts">
  import RuneTable from "../../RuneTable.svelte";

  /** "Rune Ra Fo" -> Ra / "Fo"; "Rune Fo" -> Base / "Fo". Special runes ("Rune de chasse", "Rune de Signature"...) are left out. */
  const parse = (n: string) => {
    const m = n.match(/^Rune (?:(Pa|Ra) )?(?!de )(.+)$/);
    return m ? { tier: m[1] ?? "Base", stat: m[2] } : null;
  };
</script>

<RuneTable kind="forge" title="Runes" tiers={["Base", "Pa", "Ra"]} {parse} />
