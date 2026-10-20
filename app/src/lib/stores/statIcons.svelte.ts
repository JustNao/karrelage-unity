import { call } from "../api";
import { STAT_ICON } from "../stuff";

/**
 * Dofusbook stat icons (icon code -> data URL, downloaded once and cached on disk by the stuff module), shared by every view:
 * fetched the first time an icon is asked for. `iconOfStat(stat)` is reactive (re-renders once the icons are in).
 */
let icons = $state.raw<Record<string, string>>({});
let asked = false;

export function iconOfStat(stat: number | null | undefined): string | undefined {
  if (!asked) {
    asked = true;
    call<Record<string, string>>("stuff", "stat_icons")
      .then((m) => (icons = m))
      .catch(() => (asked = false));
  }
  if (stat == null) return undefined;
  const code = STAT_ICON.get(stat);
  return code ? icons[code] : undefined;
}
