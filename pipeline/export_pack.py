"""Export the app's data pack -> datapack/ (small, build-specific; the Tauri app loads it instead of work/).
Needs: work/pb/protocol.desc (build_protos.py), work/gamedata (extract_gamedata.py), work/mapping.json + overrides.json."""
import csv, json, pathlib, re, shutil, time, sys, unicodedata
import schema, gamedata as gd

root = pathlib.Path(__file__).resolve().parent.parent
out = root / "datapack"
out.mkdir(exist_ok=True)


def dump(name, obj):
    (out / name).write_text(json.dumps(obj, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")


shutil.copy(root / "work" / "pb" / "protocol.desc", out / "protocol.desc")
shutil.copy(pathlib.Path(__file__).with_name("bindings.json"), out / "bindings.json")

# names: message -> {name, conf, fields:{obf: {name|cands}}}   (mapping.json with overrides applied)
dump("names.json", schema.mapping())

ids = set()
items = {}
for k, v in gd.items.items():
    items[k] = {"nameId": v["nameId"], "typeId": v["typeId"], "level": v["level"], "iconId": v["iconId"], "setId": v.get("itemSetId"), "drop": bool(v.get("drop")),
                "effects": [{"a": e["a"], "min": e["min"], "max": e["max"]} for e in v.get("effects", [])]}
    ids.add(v["nameId"])
# Effect classification, following the in-game characteristics panel:
#   stats (vitality, force, ...) / main (AP, MP, range, ...) / secondary (tackle, initiative, ...) / damage / resist / other,
# plus "weapon": the effects of weapons (category 2: hits "dommages X", "vole X", heals, -PA, pushback...). `density` = effectPowerRate (rune weight
# per unit: AP 100, MP 90, vitality 0.2, ...), kept for later features.
STATS = ["vitality", "wisdom", "strength", "intelligence", "chance", "agility"]
MAIN = ["actionPoints", "movementPoints", "range", "criticalHit", "damagePercent", "maxSummonedCreaturesBoost"]


def classify(e):
    ch = gd.characteristics.get(e["characteristic"]) if e["characteristic"] else None
    if e["category"] == 2:   # hits first ("dommages X", "vole X"), then -PA / -PM..., then flags (pushback...)
        return "weapon", 0 if e["useDice"] and not e["characteristic"] else 1 if e["useDice"] else 2
    if not ch:
        return "other", 0
    kw = ch["keyword"]
    if kw in STATS:
        return "stats", STATS.index(kw)
    if kw in MAIN:
        return "main", MAIN.index(kw)
    return {3: "secondary", 4: "damage", 5: "resist"}.get(ch["categoryId"], "other"), 100 + ch["order"]


effects, ranked = {}, {}
for k, v in gd.effects.items():
    group, rank = classify(v)
    effects[k] = {"descriptionId": v["descriptionId"], "characteristic": v["characteristic"], "useDice": v["useDice"],
                  "group": group, "density": v["effectPowerRate"]}
    ranked.setdefault(group, []).append(((rank, v["effectPowerRate"] < 0, k), k))   # positives before their negative twins
    ids.add(v["descriptionId"])
for items_ in ranked.values():
    for i, (key, k) in enumerate(sorted(items_)):
        # weapon effects: tier * 1000 + rank (tier 0 = hits, 1 = -PA..., 2 = flags); the UI sorts the hits by value
        effects[k]["order"] = key[0] * 1000 + i if effects[k]["group"] == "weapon" else i
types = {k: {"nameId": v["nameId"], "categoryId": v["categoryId"]} for k, v in gd.itemtypes.items()}
ids.update(v["nameId"] for v in types.values())
spells = {k: {"nameId": v["nameId"]} for k, v in gd.spells.items()}
ids.update(v["nameId"] for v in spells.values())
dump("spells.json", spells)
dump("recipes.json", {k: v for k, v in gd.recipes.items() if k in items})
dump("itemsets.json", {k: v for k, v in gd.itemsets.items() if any(i in items for i in v["items"])})
ids.update(v["nameId"] for v in gd.itemsets.values() if v.get("nameId"))
dump("items.json", items); dump("effects.json", effects); dump("itemtypes.json", types)


# monsters: name + grade levels, plus the fight notes of work/monster_notes.csv (local export of the author's "Strats monstres"
# sheet: name, warning, invulnerable, tips; a cell may hold several lines). Notes are matched by name (accents / case ignored) and
# apply to every monster of that name (variants share it).
def norm(s):
    s = unicodedata.normalize("NFKD", s)
    return re.sub(r"[^a-z0-9 ]", "", "".join(c for c in s if not unicodedata.combining(c)).lower()).strip()


monsters = {k: {"nameId": v["nameId"], "levels": v["levels"]} for k, v in gd.monsters.items()}
ids.update(v["nameId"] for v in monsters.values())
notes_csv = root / "work" / "monster_notes.csv"
if notes_csv.exists() and monsters:
    by_name = {}
    for k, v in monsters.items():
        by_name.setdefault(norm(gd.text(v["nameId"]) or ""), []).append(k)
    noted, unmatched = 0, []
    with open(notes_csv, newline="", encoding="utf-8-sig") as f:
        for row in csv.DictReader(f):
            note = {k: lines for k in ("warning", "invulnerable", "tips") if (lines := [l.strip() for l in (row.get(k) or "").splitlines() if l.strip()])}
            if not note:
                continue
            ks = by_name.get(norm(row["name"]))
            if not ks:
                unmatched.append(row["name"])
                continue
            noted += 1
            for k in ks:
                for kind, lines in note.items():
                    monsters[k].setdefault("notes", {}).setdefault(kind, []).extend(lines)
    print(f"monster notes: {noted} names matched", f"unmatched: {unmatched}" if unmatched else "")
# DofusDB tags (what a monster's spells do: push, pull, retPM, poison, summon, elements...) + flags, from the local
# work/dofusdb_monsters.json; flags become tags too: boss, miniBoss, and noPush / noSwitch / noCarry / noTackle when it cannot be.
DB_FLAGS = {"isBoss": ("boss", True), "isMiniBoss": ("miniBoss", True), "canBePushed": ("noPush", False),
            "canSwitchPos": ("noSwitch", False), "canBeCarried": ("noCarry", False), "canTackle": ("noTackle", False)}
db_file = root / "work" / "dofusdb_monsters.json"
if db_file.exists():
    db = {int(k): v for k, v in json.loads(db_file.read_text(encoding="utf-8")).items()}
    for k, v in monsters.items():
        d = db.get(k)
        if d:
            tags = list(d.get("tags") or []) + [t for f, (t, when) in DB_FLAGS.items() if d.get(f) is when]
            if tags:
                v["tags"] = tags
    print("monster tags:", sum(1 for v in monsters.values() if v.get("tags")), "monsters")
dump("monsters.json", monsters)

for lang in ("fr", "en"):
    f = gd.D / "i18n" / f"{lang}.bin"
    if f.exists():
        i = gd.I18n(f)
        dump(f"strings.{lang}.json", {str(n): s for n in sorted(ids) if (s := i.get(n))})

# fingerprint of the game build the protos were dumped from (sizes of the copied files + launcher version): the app compares it
# with the installed game
from game_version import dumped_version
fp = {}
for key, name in (("assembly", "GameAssembly.dll"), ("metadata", "global-metadata.dat")):
    f = root / "work" / "dump_input" / name
    if f.exists():
        fp[key] = f.stat().st_size
if v := dumped_version():
    fp["version"] = v
dump("manifest.json", {"pack": 1, "generated": int(time.time()), "items": len(items), "effects": len(effects),
                       "messages": len(schema.mapping()), "game": fp})
print("datapack:", {p.name: p.stat().st_size for p in out.iterdir()})
