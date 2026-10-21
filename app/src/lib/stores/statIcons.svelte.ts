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

/** Elemental damage stats (Do Neutre / Terre / Feu / Eau / Air), whose icons stand for the element. */
const ELEMENT_STAT: Record<string, number> = { neutre: 92, terre: 88, feu: 89, eau: 90, air: 91 };

/**
 * Icon of the element of a weapon hit ("12 à 15 dommages Feu", "vol Eau", "Dommages Air : 8% PV de la cible"), from its
 * text; none for heals or the best / worst element.
 */
export function iconOfElement(text: string | null | undefined): string | undefined {
  const m = /\b(?:dommages|vol)\s+(neutre|terre|feu|eau|air)\b/i.exec(text ?? "");
  return m ? iconOfStat(ELEMENT_STAT[m[1].toLowerCase()]) : undefined;
}
