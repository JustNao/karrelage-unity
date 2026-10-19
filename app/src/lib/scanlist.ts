/** The floating "items to scan" window (scanlist.html, src/ScanList.svelte) is fed through this shared localStorage key. */
export const SCAN_LIST_KEY = "karrelage.scanlist";
export interface ScanItem { gid: number; name: string }
