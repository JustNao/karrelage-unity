/**
 * Resizable columns, the default layout for tabs with side-by-side panels: a grid whose panels are separated by thin
 * draggable grips. Widths are in px, remembered between sessions; the flexible column (1fr) takes what is left.
 *
 *   const cols = new Columns("loutil.cols.<tab>", { left: 280, right: 470 });
 *   <div class="grid" style:grid-template-columns="{cols.w.left}px 5px 1fr 5px {cols.w.right}px"> panel, <div class="grip" onpointerdown={(e) => cols.drag("left", 1, e)}>, ...
 *
 * `dir` is +1 when the panel sits on the left of the grip (grows with the pointer), -1 when on its right. Style `.grip`
 * with `.grip` from app.css.
 */
export class Columns<K extends string> {
  w = $state() as Record<K, number>;

  constructor(
    private key: string,
    defaults: Record<K, number>,
    private min = 100,
    private max = 900,
  ) {
    this.w = { ...defaults };
    try {
      Object.assign(this.w, JSON.parse(localStorage.getItem(key) ?? "{}"));
    } catch {}
  }

  drag(name: K, dir: 1 | -1, e: PointerEvent) {
    const x0 = e.clientX;
    const w0 = this.w[name];
    const move = (ev: PointerEvent) => (this.w[name] = Math.min(this.max, Math.max(this.min, w0 + dir * (ev.clientX - x0))));
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      try {
        localStorage.setItem(this.key, JSON.stringify(this.w));
      } catch {}
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    e.preventDefault();
  }
}
