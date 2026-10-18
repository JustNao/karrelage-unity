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
TABLES = ["items", "effects", "characteristics", "itemtypes", "itemsets", "itemsupertypes"]
UnityPy.config.FALLBACK_UNITY_VERSION = "6000.3.16f1"   # bundles ship with a stripped version; matches UnityPlayer.dll


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
    for n, d in (("items", items), ("effects", effects), ("characteristics", chars), ("itemtypes", types)):
        (OUT / f"{n}.json").write_text(json.dumps(d, ensure_ascii=False), encoding="utf-8")
        print(n, len(d))


if __name__ == "__main__":
    main()
