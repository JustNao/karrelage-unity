/**
 * Svelte action for lists in a box of fixed height with no scroll bar (Crafting « Accueil » cards): the children that do not
 * fit whole are hidden, from the first one that overflows on. Followed on resizes and when the children change.
 *
 *   <div class="list" use:fitRows>{#each rows as r}<div class="row">…</div>{/each}</div>   (.list: flex: 1; min-height: 0)
 */
export function fitRows(node: HTMLElement) {
  node.style.overflow = "hidden";
  if (getComputedStyle(node).position === "static") node.style.position = "relative"; // children's offsetTop from the node
  let frame = 0;
  const fit = () => {
    frame = 0;
    const kids = [...node.children] as HTMLElement[];
    for (const k of kids) k.style.display = "";
    const room = node.clientHeight;
    let full = false;
    for (const k of kids) {
      full ||= k.offsetTop + k.offsetHeight > room + 0.5;
      if (full) k.style.display = "none";
    }
  };
  const soon = () => (frame ||= requestAnimationFrame(fit));
  const sized = new ResizeObserver(soon);
  sized.observe(node);
  const changed = new MutationObserver(soon);
  changed.observe(node, { childList: true, subtree: true, characterData: true });
  soon();
  return {
    destroy() {
      sized.disconnect();
      changed.disconnect();
      cancelAnimationFrame(frame);
    },
  };
}
