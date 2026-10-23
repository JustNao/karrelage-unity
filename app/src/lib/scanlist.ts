import { openFloating } from "./floating";
import { writeJSON } from "./persist.svelte";

/** The floating "items to scan" window (scanlist.html, src/ScanList.svelte) is fed through this shared localStorage key. */
export const SCAN_LIST_KEY = "karrelage.scanlist";
/** group: equipment (weapons, armour, rings, Dofus...) or companion (familiar / mount). */
export type ScanGroup = "equip" | "companion";
export interface ScanItem { gid: number; name: string; group: ScanGroup }

/** Show these items in the floating scan list (an open window picks the new list up through the storage event). */
export async function openScanList(items: ScanItem[]) {
  writeJSON(SCAN_LIST_KEY, items);
  await openFloating("scanlist", { url: "scanlist.html", title: "À scanner", width: 280, height: 380 });
}
