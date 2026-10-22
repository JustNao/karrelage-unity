/**
 * Monster tags (data pack monsters.json `tags`, from DofusDB): what a monster's spells do, plus flags. Grouped for display:
 * flag (boss, cannot be pushed...), control (push, pull, AP / MP removal, poison... what makes a fight dangerous),
 * elem (its damage elements), boost (what it buffs). Unknown tags fall in "boost" under their raw name.
 */
export type TagGroup = "flag" | "control" | "elem" | "boost";

export const TAGS: Record<string, { label: string; group: TagGroup }> = {
  boss: { label: "Boss", group: "flag" },
  miniBoss: { label: "Archimonstre", group: "flag" },
  noPush: { label: "Non poussable", group: "flag" },
  noSwitch: { label: "Non échangeable", group: "flag" },
  noCarry: { label: "Non portable", group: "flag" },
  noTackle: { label: "Ne tacle pas", group: "flag" },

  push: { label: "Poussée", group: "control" },
  pull: { label: "Attirance", group: "control" },
  tp: { label: "Téléportation", group: "control" },
  retPA: { label: "Retrait PA", group: "control" },
  retPM: { label: "Retrait PM", group: "control" },
  retPO: { label: "Retrait PO", group: "control" },
  poison: { label: "Poison", group: "control" },
  erode: { label: "Érosion", group: "control" },
  malusRes: { label: "-% résistances", group: "control" },
  malusDmg: { label: "-dommages", group: "control" },
  debuff: { label: "Désenvoûtement", group: "control" },
  lifeSteal: { label: "Vol de vie", group: "control" },
  heal: { label: "Soins", group: "control" },
  summon: { label: "Invocateur", group: "control" },
  glyph: { label: "Glyphes", group: "control" },
  trap: { label: "Pièges", group: "control" },

  neutral: { label: "Neutre", group: "elem" },
  earth: { label: "Terre", group: "elem" },
  fire: { label: "Feu", group: "elem" },
  water: { label: "Eau", group: "elem" },
  air: { label: "Air", group: "elem" },
  multi: { label: "Multi-élément", group: "elem" },
  bestElem: { label: "Meilleur élément", group: "elem" },

  boostAP: { label: "PA", group: "boost" },
  boostMP: { label: "PM", group: "boost" },
  boostPO: { label: "PO", group: "boost" },
  boostPower: { label: "Puissance", group: "boost" },
  boostShield: { label: "Bouclier", group: "boost" },
  boostElemDmg: { label: "Dommages élémentaires", group: "boost" },
  boostCC: { label: "Critique", group: "boost" },
  boostCCDmg: { label: "Dommages critiques", group: "boost" },
  boostDodge: { label: "Fuite", group: "boost" },
  boostTackle: { label: "Tacle", group: "boost" },
  boostVitality: { label: "Vitalité", group: "boost" },
  boostHeal: { label: "Soins", group: "boost" },
  boostPushDmg: { label: "Dommages de poussée", group: "boost" },
  boostFinalDmg: { label: "Dommages finaux", group: "boost" },
  boostMeleeDmg: { label: "Dommages mêlée", group: "boost" },
  boostDistanceDmg: { label: "Dommages distance", group: "boost" },
  boostStrength: { label: "Force", group: "boost" },
  boostIntelligence: { label: "Intelligence", group: "boost" },
  boostChance: { label: "Chance", group: "boost" },
  boostAgility: { label: "Agilité", group: "boost" },
  boostWisdom: { label: "Sagesse", group: "boost" },
  boostRetAP: { label: "Retrait PA", group: "boost" },
  boostRetMP: { label: "Retrait PM", group: "boost" },
  boostEsqAP: { label: "Esquive PA", group: "boost" },
  boostEsqMP: { label: "Esquive PM", group: "boost" },
  boostPushRes: { label: "Rés. poussée", group: "boost" },
  boostElemRes: { label: "Résistances", group: "boost" },
  boostCCRes: { label: "Rés. critiques", group: "boost" },
  boostDistanceRes: { label: "Rés. distance", group: "boost" },
  boostSpellRes: { label: "Rés. sorts", group: "boost" },
  boostSpellHeal: { label: "Soins des sorts", group: "boost" },
  boostSummons: { label: "Invocations", group: "boost" },
};

const ORDER: TagGroup[] = ["flag", "control", "elem", "boost"];

export interface Tag {
  id: string;
  label: string;
  group: TagGroup;
}

/** A monster's tags, labelled and in display order (flags, control, elements, boosts); `groups` keeps only those groups. */
export function tagsOf(ids: string[] | undefined, groups: TagGroup[] = ORDER): Tag[] {
  const seen = new Set<string>();
  return (ids ?? [])
    .filter((id) => !seen.has(id) && seen.add(id))
    .map((id) => ({ id, ...(TAGS[id] ?? { label: id, group: "boost" as TagGroup }) }))
    .filter((t) => groups.includes(t.group))
    .sort((a, b) => ORDER.indexOf(a.group) - ORDER.indexOf(b.group));
}
