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

const found = import.meta.glob<{ default: UiModule }>("../modules/*/index.ts", { eager: true });

/** All modules, auto-discovered: add a folder under src/modules and it shows up as a tab. */
export const modules: UiModule[] = Object.values(found)
  .map((m) => m.default)
  .sort((a, b) => a.order - b.order);
