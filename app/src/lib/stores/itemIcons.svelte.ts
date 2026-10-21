import { SvelteMap } from "svelte/reactivity";
import { call } from "../api";

/**
 * Item icons (game iconId = Dofusbook picture id -> data URL, downloaded once and cached on disk by the stuff module),
 * shared by every view. Asked for lazily (`lazyIcon` on the element showing it) and fetched in small batches, so long
 * lists only load what scrolls into view. `iconOfItem(id)` is reactive.
 */
const icons = new SvelteMap<number, string>();
const asked = new Set<number>();
let queue: number[] = [];
let timer: ReturnType<typeof setTimeout> | null = null;
const BATCH = 8; // small batches: the first icons show up while the others download

function flush() {
  timer = null;
  while (queue.length) {
    const ids = queue.splice(0, BATCH);
    call<Record<string, string>>("stuff", "icons", { item: ids })
      .then((m) => {
        for (const id of ids) {
          const u = m[`item:${id}`];
          if (u) icons.set(id, u);
        }
      })
      .catch(() => ids.forEach((id) => asked.delete(id))); // asked again next time it is shown
  }
}

export function requestIcon(id: number | null | undefined) {
  if (id == null || asked.has(id)) return;
  asked.add(id);
  queue.push(id);
  timer ??= setTimeout(flush, 30); // gather what appears in the same frame
}

export const iconOfItem = (id: number | null | undefined): string | undefined => (id == null ? undefined : icons.get(id));

/** Svelte action: request the icon once the element is (nearly) on screen. */
export function lazyIcon(node: Element, id: number | null | undefined) {
  let cur = id;
  const io = new IntersectionObserver(
    (es) => {
      if (es.some((e) => e.isIntersecting)) {
        requestIcon(cur);
        io.disconnect();
      }
    },
    { rootMargin: "200px" },
  );
  io.observe(node);
  return {
    update(n: number | null | undefined) {
      cur = n;
      io.disconnect();
      io.observe(node);
    },
    destroy: () => io.disconnect(),
  };
}
