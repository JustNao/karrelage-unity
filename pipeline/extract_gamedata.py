"""Extract static game data from LOCAL COPIES of the client's Addressables bundles (read-only on the game).

  python extract_gamedata.py [--refresh]    # --refresh re-copies bundles/i18n from the game folder first

Reads work/gamedata/bundles/*.asset.bundle (UnityFS, ScriptableObjects with managed references) via UnityPy and writes
work/gamedata/{items,effects,characteristics,itemtypes}.json.  i18n (work/gamedata/i18n/<lang>.bin) is read lazily by gamedata.py.
"""
import sys, json, pathlib, shutil, warnings, argparse

root = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(root / "tools" / "pylib"))
warnings.filterwarnings("ignore")
import UnityPy

GAME = pathlib.Path(r"C:\Users\nao3n\AppData\Local\Ankama\Dofus-dofus3\Dofus_Data\StreamingAssets\Content")
OUT = root / "work" / "gamedata"
BUNDLES = OUT / "bundles"
TABLES = ["items", "effects", "characteristics", "itemtypes", "itemsets", "itemsupertypes", "spells", "recipes", "monsters", "npcs"]
UnityPy.config.FALLBACK_UNITY_VERSION = "6000.3.16f1"   # bundles ship with a stripped version; matches UnityPlayer.dll

LEGENDARY_SPELL_TYPE = 339   # spells.typeId of the passives of legendary items


def refresh():
    BUNDLES.mkdir(parents=True, exist_ok=True)
    (OUT / "i18n").mkdir(parents=True, exist_ok=True)
    for t in TABLES:
        shutil.copyfile(GAME / "Data" / f"data_assets_{t}dataroot.asset.bundle", BUNDLES / f"data_assets_{t}dataroot.asset.bundle")
    for lang in ("fr", "en"):
        shutil.copyfile(GAME / "I18n" / f"{lang}.bin", OUT / "i18n" / f"{lang}.bin")


def rows(name, with_refs=False):
    env = UnityPy.load(str(BUNDLES / f"data_assets_{name}dataroot.asset.bundle"))
    for o in env.objects:
        if o.type.name == "MonoBehaviour":
            refs = o.read_typetree()["references"]["RefIds"]
            out = [dict(x["data"], _class=x["type"]["class"], _rid=x["rid"]) for x in refs]
            return (out, {r["_rid"]: r for r in out}) if with_refs else out
    return ([], {}) if with_refs else []


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--refresh", action="store_true")
    if ap.parse_args().refresh:
        refresh()
    items = {}
    allrows, byrid = rows("items", with_refs=True)
    for r in allrows:
        if "id" not in r or "nameId" not in r:
            continue
        items[r["id"]] = {k: r.get(k) for k in ("id", "nameId", "descriptionId", "typeId", "level", "iconId", "itemSetId", "realWeight", "price", "_class")}
        items[r["id"]]["drop"] = bool(r.get("dropMonsterIds"))   # dropped by some monster (Temporis-only drops left out)
        # default ("perfect roll range") effects: action id == effects table id; dice num..side is the min..max (side 0 = fixed)
        eff = []
        for ref in r.get("possibleEffects", []):
            e = byrid.get(ref["rid"])
            if not e:
                continue
            n, sd = e.get("diceNum", 0), e.get("diceSide", 0)
            lo, hi = (n, sd) if sd else (n, n)
            eff.append({"a": e["actionId"], "min": min(lo, hi), "max": max(lo, hi), "v": e.get("value", 0)})
        items[r["id"]]["effects"] = eff
    effects = {r["id"]: {k: r.get(k) for k in ("id", "descriptionId", "characteristic", "category", "useDice", "isInPercent", "effectPowerRate", "showInTooltip", "elementId", "boost", "oppositeId")} for r in rows("effects")}
    chars = {r["id"]: {k: r.get(k) for k in ("id", "keyword", "nameId", "order", "categoryId")} for r in rows("characteristics")}
    types = {r["id"]: {k: r.get(k) for k in ("id", "nameId", "superTypeId", "categoryId")} for r in rows("itemtypes")}
    # legendary item passives (effect 1175 carries the spell id): only their names are needed
    # ... plus every spell an item's effect 1175 points to (the special effect of a Dofus: "Rouge Vermeil" for the Dofus Vulbis)
    referenced = {e["max"] for i in items.values() for e in i.get("effects", []) if e["a"] == 1175}
    spells = {r["id"]: {"id": r["id"], "nameId": r["nameId"]} for r in rows("spells") if (r.get("typeId") == LEGENDARY_SPELL_TYPE or r.get("id") in referenced) and "nameId" in r}
    # crafting recipes: result item -> list of recipes, each a list of [ingredient gid, quantity]
    recipes = {}
    for r in rows("recipes"):
        if "resultId" in r:
            recipes.setdefault(r["resultId"], []).append([[i, q] for i, q in zip(r["ingredientIds"], r["quantities"])])
    # item sets: bonuses per piece count (effects[i] = bonus when i+1 pieces are worn), effect = {a: action id, min, max} like item defaults
    sets = {}
    srows, sby = rows("itemsets", with_refs=True)
    for r in srows:
        if r.get("_class") != "ItemSetData" or "items" not in r:
            continue
        bonus = []
        for lvl in r.get("effects", []):
            lst = []
            for ref in lvl.get("values", []):
                e = sby.get(ref["rid"])
                if e and "actionId" in e:
                    n, sd = e.get("diceNum", 0), e.get("diceSide", 0)
                    lst.append({"a": e["actionId"], "min": min(n, sd or n), "max": max(n, sd or n)})
            bonus.append(lst)
        sets[r["id"]] = {"id": r["id"], "nameId": r.get("nameId"), "items": r["items"], "effects": bonus}
    # monsters (a fighter's creature gid): name + level of each grade (resolves the unordered gid / grade / level ints of a fighter)
    monsters = {r["id"]: {"id": r["id"], "nameId": r["nameId"], "race": r.get("race"), "levels": [g.get("level", 0) for g in r.get("grades", [])]}
                for r in rows("monsters") if "id" in r and "nameId" in r}
    # NPCs (an npc actor of a map: Songes goblins...): name only
    npcs = {r["id"]: {"id": r["id"], "nameId": r["nameId"]} for r in rows("npcs") if "id" in r and r.get("nameId")}
    for n, d in (("items", items), ("effects", effects), ("characteristics", chars), ("itemtypes", types), ("spells", spells), ("recipes", recipes), ("itemsets", sets), ("monsters", monsters), ("npcs", npcs)):
        (OUT / f"{n}.json").write_text(json.dumps(d, ensure_ascii=False), encoding="utf-8")
        print(n, len(d))


if __name__ == "__main__":
    main()
