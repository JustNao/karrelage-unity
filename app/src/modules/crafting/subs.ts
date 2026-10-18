import type { Component } from "svelte";

/** A crafting sub-module (src/modules/crafting/subs/<id>/index.ts): one entry in the left column of the Crafting tab. */
export interface CraftSub {
  id: string;
  title: string;
  description: string;
  order: number;
  component: Component;
}

const found = import.meta.glob<{ default: CraftSub }>("./subs/*/index.ts", { eager: true });

export const subs: CraftSub[] = Object.values(found)
  .map((m) => m.default)
  .sort((a, b) => a.order - b.order);
