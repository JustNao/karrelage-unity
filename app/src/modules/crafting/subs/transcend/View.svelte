<script lang="ts">
  import RuneTable from "../../RuneTable.svelte";

  const parse = (n: string) => {
    const m = n.match(/^Rune (Ta|Pata|Rata) (.+)$/);
    return m ? { tier: m[1], stat: m[2] } : null;
  };
</script>

<RuneTable kind="transcend" title="Runes de transcendance" tiers={["Ta", "Pata", "Rata"]} {parse} />
