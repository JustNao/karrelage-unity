import type { Component } from "svelte";

/** What a UI module (src/modules/<id>/index.ts) exports as default. */
export interface UiModule {
  id: string;
  title: string;
  order: number;
  /** Hidden unless developer mode is on. */
  devOnly?: boolean;
  component: Component;
}

/** The default exports of an eager `import.meta.glob` of index.ts files, sorted by `order`: tabs and sub-tabs register
 *  themselves by existing. */
export function discover<T extends { order: number }>(found: Record<string, { default: T }>): T[] {
  return Object.values(found)
    .map((m) => m.default)
    .sort((a, b) => a.order - b.order);
}

/** All modules, auto-discovered: add a folder under src/modules and it shows up as a tab. */
export const modules: UiModule[] = discover(import.meta.glob<{ default: UiModule }>("../modules/*/index.ts", { eager: true }));
