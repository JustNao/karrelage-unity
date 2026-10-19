/** Sortable table state shared by every table (SortTable.svelte): click a column to sort by it, again to flip the order;
 *  unknown values (null, NaN, ±Infinity) always go last, whatever the direction. */

const collator = new Intl.Collator("fr");

export class Sort<K extends string> {
  /** null = the rows keep the order they were given in. */
  key = $state<K | null>(null);
  desc = $state(false);

  /**
   * `defaultKey` / `defaultDesc`: the starting sort. `descFirst(k)`: whether a column starts descending when it is first
   * clicked (e.g. profits: best first); defaults to `defaultDesc` for every column.
   */
  constructor(
    defaultKey: K | null = null,
    defaultDesc = false,
    private descFirst: (k: K) => boolean = () => defaultDesc,
  ) {
    this.key = defaultKey;
    this.desc = defaultDesc;
  }

  set(k: K) {
    if (this.key === k) this.desc = !this.desc;
    else {
      this.key = k;
      this.desc = this.descFirst(k);
    }
  }
  /** " ▾" / " ▴" next to the sorted column's label. */
  arrow(k: K) {
    return this.key === k ? (this.desc ? " ▾" : " ▴") : "";
  }
  /** A sorted copy of `rows` (`value` reads a column of a row; by default the field of the same name). */
  apply<R>(rows: R[], value: (r: R, k: K) => unknown = (r, k) => (r as any)[k]): R[] {
    const k = this.key;
    if (!k) return rows;
    const desc = this.desc;
    const bad = (v: unknown) => v == null || (typeof v === "number" && !Number.isFinite(v));
    return [...rows].sort((a, b) => {
      const x = value(a, k), y = value(b, k);
      if (bad(x) && bad(y)) return 0;
      if (bad(x)) return 1;
      if (bad(y)) return -1;
      const d = typeof x === "string" ? collator.compare(x, y as string) : (x as number) - (y as number);
      return desc ? -d : d;
    });
  }
}

/** `const sort = useSort<"name" | "price">("price", true)`. */
export const useSort = <K extends string>(defaultKey: K | null = null, defaultDesc = false, descFirst?: (k: K) => boolean) => new Sort<K>(defaultKey, defaultDesc, descFirst);
