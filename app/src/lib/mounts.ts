/** Mount extraction (dragodindes, muldos, volkornes; tables in modules/crafting/subs/montures/mounts.json): shared by the Crafting tab and the HDV tab. */
import { call } from "./api";
import table from "../modules/crafting/subs/montures/mounts.json";

export interface Family {
  id: string;
  title: string;
  /** Resource obtained by extracting a mount of this family. */
  neurone: string;
  /** [game item name, generation, resources per extraction]. */
  mounts: [string, number, number][];
}
export const FAMILIES = table.families as unknown as Family[];

/** Name of the resource of a family, plural ("Neurone de dragodinde" -> "Neurones"). */
export const resourcePlural = (neurone: string) => neurone.split(" ")[0] + "s";

const MOUNTS = new Map(FAMILIES.flatMap((f) => f.mounts.map(([name, gen, neurons]) => [name, { gen, neurons, neurone: f.neurone, family: f.id }] as const)));

/** Generation, yield and resource of extracting this mount (null = not in the tables). */
export const mountInfo = (name: string | undefined) => (name ? (MOUNTS.get(name) ?? null) : null);

/** Effect carried by senile lots only: a senile mount gives a single resource whatever its generation. */
export const SENILE_EFFECT = 3836;
export const isSenile = (lot: any) => lot.effects.some((e: any) => e.action === SENILE_EFFECT && e.status !== "removed");

/** Resources obtained by extracting this lot (`neurons` = the generation's yield). */
export const neuronsOf = (lot: any, neurons: number) => (isSenile(lot) ? 1 : neurons);

let gids: Promise<Record<string, number>> | null = null;
/** Resource name -> item gid, for every family (cached). */
export const neuroneGids = () => (gids ??= call<Record<string, number>>("prices", "gids_by_name", { names: FAMILIES.map((f) => f.neurone) }).catch((e) => ((gids = null), Promise.reject(e))));
