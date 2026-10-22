/**
 * User settings (Paramètres tab), one section per tab, remembered in localStorage `karrelage.settings`. Reactive: views
 * and helpers read `settings.hdv.goodZ` and follow changes at once.
 *
 * To add a setting: give it a default in DEFAULTS (missing keys of older saves fall back to it) and a field in the tab.
 */
const KEY = "karrelage.settings";

const DEFAULTS = {
  hdv: {
    /** A lot this many residual standard deviations above the price / density trend is a good deal. */
    goodZ: 2,
  },
  combat: {
    /** Follow the monsters of the current fight (off: the fight messages are not even decoded). */
    enabled: true,
    /** Notes shown per monster: warnings only, + invulnerabilities, + tips. */
    show: "all" as CombatShow,
    /** Open the Combat tab when a fight starts. */
    autoOpen: false,
  },
};
export type CombatShow = "warning" | "invulnerable" | "all";
export type Settings = typeof DEFAULTS;
export const SETTING_DEFAULTS: Settings = DEFAULTS;

function load(): Settings {
  const out = structuredClone(DEFAULTS);
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    for (const s of Object.keys(out) as (keyof Settings)[]) Object.assign(out[s], saved?.[s] ?? {});
  } catch {}
  return out;
}

export const settings = $state<Settings>(load());

$effect.root(() => {
  $effect(() => {
    const json = JSON.stringify(settings); // read deeply: every change saves
    try {
      localStorage.setItem(KEY, json);
    } catch {}
  });
});
