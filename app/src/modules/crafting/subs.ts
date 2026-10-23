import { discover, type UiModule } from "../../lib/registry";

/** A crafting sub-module (src/modules/crafting/subs/<id>/index.ts): one entry in the left column of the Crafting tab. */
export interface CraftSub extends Omit<UiModule, "devOnly"> {
  description: string;
}

export const subs: CraftSub[] = discover(import.meta.glob<{ default: CraftSub }>("./subs/*/index.ts", { eager: true }));
