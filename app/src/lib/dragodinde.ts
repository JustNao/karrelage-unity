/** Dragodinde extraction into neurones (table from the guide, see dragodindes.json): shared by the Crafting tab and the HDV tab. */
import { call } from "./api";
import table from "../modules/crafting/subs/dragodindes/dragodindes.json";

export const NEURONE: string = table.neurone;
const MOUNTS = new Map((table.mounts as [string, number, number][]).map(([name, gen, neurons]) => [name, { gen, neurons }]));

/** Generation and neurones obtained by extracting this mount (null = not in the table). */
export const mountInfo = (name: string | undefined) => (name ? (MOUNTS.get(name) ?? null) : null);

/** Effect carried by senile lots only: a senile mount gives a single neurone whatever its generation. */
export const SENILE_EFFECT = 3836;
export const isSenile = (lot: any) => lot.effects.some((e: any) => e.action === SENILE_EFFECT && e.status !== "removed");

/** Neurones obtained by extracting this lot (`neurons` = the generation's yield). */
export const neuronsOf = (lot: any, neurons: number) => (isSenile(lot) ? 1 : neurons);

let gid: Promise<number | null> | null = null;
/** Item gid of the neurone (cached). */
export const neuroneGid = () => (gid ??= call<Record<string, number>>("prices", "gids_by_name", { names: [NEURONE] }).then((m) => m[NEURONE] ?? null).catch(() => ((gid = null), null)));
