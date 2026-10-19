/** The floating "items to scan" window (scanlist.html, src/ScanList.svelte) is fed through this shared localStorage key. */
export const SCAN_LIST_KEY = "karrelage.scanlist";
/** group: equipment (weapons, armour, rings, Dofus...) or companion (familiar / mount). */
export type ScanGroup = "equip" | "companion";
export interface ScanItem { gid: number; name: string; group: ScanGroup }
