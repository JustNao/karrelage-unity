/** HDV effect filters: per scanned item, a min / max per effect (kept for the session), and the operations of the filter panel. */
import { FLAG_EFFECTS, type EffectMeta } from "../../lib/effects";

export interface Filter { a: number; min: string | number; max: string | number }
/** A filter row of the panel: an effect and its default range (null = an effect the user added). */
export interface EffectRow { a: number; def: number[] | null }

/** Highest value of an effect on a lot (0 when it lacks it). */
export const lotVal = (r: any, a: number) => Math.max(0, ...r.effects.filter((e: any) => e.action === a && e.status !== "removed").map((e: any) => e.hi ?? 0));
export const effectLabel = (meta: Record<number, EffectMeta>, a: number) => meta[a]?.label ?? `effect ${a}`;

export class Filters {
  /** gid -> filters */
  byGid = $state<Record<number, Filter[]>>({});

  /** `item()`: the open item (the panel's operations act on it). */
  constructor(private item: () => any) {}

  list(gid: number): Filter[] {
    return this.byGid[gid] ?? [];
  }

  /** The filter of an effect of an item, created empty when missing (the reactive proxy, so edits update the views). */
  of(gid: number, a: number): Filter {
    this.byGid[gid] ??= [];
    const f = this.byGid[gid]; // re-read: `??=` yields the raw array, not the reactive proxy
    let x = f.find((y) => y.a === a);
    if (!x) {
      f.push({ a, min: "", max: "" });
      x = f[f.length - 1];
    }
    return x;
  }

  reset(gid: number) {
    this.byGid[gid] = [];
  }

  /** The lot passes every filter of its item. */
  matches(gid: number, r: any): boolean {
    return this.list(gid)
      .filter((x) => x.min !== "" || x.max !== "")
      .every((x) => {
        if (FLAG_EFFECTS.has(x.a)) return x.min === "" || r.effects.some((e: any) => e.action === x.a && e.status !== "removed");
        const v = lotVal(r, x.a);
        return (x.min === "" || v >= +x.min) && (x.max === "" || v <= +x.max);
      });
  }

  active(a: number): boolean {
    const it = this.item();
    const y = it && this.list(it.gid).find((z) => z.a === a);
    return !!y && (y.min !== "" || y.max !== "");
  }

  clear(a: number) {
    const it = this.item();
    if (!it) return;
    const y = this.of(it.gid, a);
    y.min = "";
    y.max = "";
  }

  /** Fill the min of one effect: the low end of its default range (or the lowest value seen in the lots); flags: lots that have it. */
  setMin(a: number, def: number[] | null) {
    const it = this.item();
    if (!it) return;
    if (FLAG_EFFECTS.has(a)) return void (this.of(it.gid, a).min = "1");
    const seen = it.rows.map((r: any) => lotVal(r, a)).filter((v: number) => v > 0);
    const v = def ? def[0] : seen.length ? Math.min(...seen) : null;
    if (v !== null) this.of(it.gid, a).min = v;
  }

  /** Fill the min with the top of the default range (or the highest value seen): only perfect rolls remain. */
  setPerf(a: number, def: number[] | null) {
    const it = this.item();
    if (!it) return;
    const seen = it.rows.map((r: any) => lotVal(r, a)).filter((v: number) => v > 0);
    const v = def ? def[1] : seen.length ? Math.max(...seen) : null;
    if (v !== null) this.of(it.gid, a).min = v;
  }

  /** Clicking an effect: fill its min, or clear it when it already filters. */
  toggle(a: number, def: number[] | null) {
    if (this.active(a)) this.clear(a);
    else this.setMin(a, def);
  }

  /** Clicking a section title: the same for all its effects. */
  toggleGroup(g: { items: EffectRow[] }) {
    if (g.items.some((x) => this.active(x.a))) g.items.forEach((x) => this.clear(x.a));
    else g.items.forEach((x) => this.setMin(x.a, x.def));
  }

  /** Preset buttons: "min" (low end of every default range), a group ("damage", "resist": perfect rolls), "clear". */
  preset(p: string, rows: EffectRow[], meta: Record<number, EffectMeta>) {
    const it = this.item();
    if (!it) return;
    if (p === "clear") return this.reset(it.gid);
    for (const x of rows) {
      if (!x.def) continue; // presets only touch the item's default effects (perfect = default max)
      const y = this.of(it.gid, x.a);
      if (p === "min") y.min = x.def[0];
      else if (p === meta[x.a]?.group) y.min = x.def[1];
    }
  }
}
