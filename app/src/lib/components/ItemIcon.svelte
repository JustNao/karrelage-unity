<script lang="ts">
  import { iconOfItem, lazyIcon } from "../stores/itemIcons.svelte";

  /** An item's icon (game iconId), fetched once it scrolls into view (lib/stores/itemIcons.svelte.ts). The box keeps its
   *  size while the icon loads; the caller sizes it through `class`. */
  let { icon, class: cls = "" }: { icon: number | null | undefined; class?: string } = $props();
</script>

<span class="ii {cls}" use:lazyIcon={icon}>{#if iconOfItem(icon)}<img src={iconOfItem(icon)} alt="" />{/if}</span>

<style>
  .ii { display: inline-block; flex: none; position: relative; }
  .ii img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: contain; }
</style>
