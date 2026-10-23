/**
 * Svelte action for popups and dropdowns: `close` runs on a pointer press outside the element (and outside `ignore`, e.g.
 * the button that toggles it) or on Escape. Put it on the popup itself, or on a wrapper holding the toggle and the popup.
 *
 *   <div class="pop" use:clickOutside={() => (open = false)}>
 *   <div class="pop" use:clickOutside={{ close: () => (pop = null), ignore: ".slot .ic" }}>
 */
type Options = (() => void) | { close: () => void; ignore?: string };

export function clickOutside(node: HTMLElement, opts: Options) {
  let o = opts;
  const close = () => (typeof o === "function" ? o() : o.close());
  const down = (e: PointerEvent) => {
    const t = e.target;
    if (!(t instanceof Node) || node.contains(t)) return;
    const ignore = typeof o === "function" ? undefined : o.ignore;
    if (ignore && t instanceof Element && t.closest(ignore)) return;
    close();
  };
  const key = (e: KeyboardEvent) => e.key === "Escape" && close();
  window.addEventListener("pointerdown", down);
  window.addEventListener("keydown", key);
  return {
    update: (n: Options) => (o = n),
    destroy() {
      window.removeEventListener("pointerdown", down);
      window.removeEventListener("keydown", key);
    },
  };
}
