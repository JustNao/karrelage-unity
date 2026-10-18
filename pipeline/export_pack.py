"""Export the app's data pack -> datapack/ (small, build-specific; the Tauri app loads it instead of work/).
Needs: work/pb/protocol.desc (build_protos.py), work/gamedata (extract_gamedata.py), work/mapping.json + overrides.json."""
import json, pathlib, shutil, time, sys
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
    items[k] = {"nameId": v["nameId"], "typeId": v["typeId"], "level": v["level"], "iconId": v["iconId"],
                "effects": [{"a": e["a"], "min": e["min"], "max": e["max"]} for e in v.get("effects", [])]}
    ids.add(v["nameId"])
effects = {}
for k, v in gd.effects.items():
    effects[k] = {"descriptionId": v["descriptionId"], "characteristic": v["characteristic"], "useDice": v["useDice"]}
    ids.add(v["descriptionId"])
types = {k: {"nameId": v["nameId"], "categoryId": v["categoryId"]} for k, v in gd.itemtypes.items()}
ids.update(v["nameId"] for v in types.values())
dump("items.json", items); dump("effects.json", effects); dump("itemtypes.json", types)

for lang in ("fr", "en"):
    f = gd.D / "i18n" / f"{lang}.bin"
    if f.exists():
        i = gd.I18n(f)
        dump(f"strings.{lang}.json", {str(n): s for n in sorted(ids) if (s := i.get(n))})

dump("manifest.json", {"pack": 1, "generated": int(time.time()), "items": len(items), "effects": len(effects),
                       "messages": len(schema.mapping())})
print("datapack:", {p.name: p.stat().st_size for p in out.iterdir()})
