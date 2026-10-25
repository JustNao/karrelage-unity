"""Lookups over data extracted by extract_gamedata.py + the client's i18n .bin (read from the local copy)."""
import json, struct, re, pathlib

root = pathlib.Path(__file__).resolve().parent.parent
D = root / "work" / "gamedata"
LANG = "fr"


class I18n:
    """<lang>.bin layout: u8 ver, pstr lang, u32 N, N*(u32 id, u32 offset), named-key table, then varint-length UTF-8 strings."""

    def __init__(self, path):
        self.d = pathlib.Path(path).read_bytes()
        n = struct.unpack_from("<I", self.d, 3)[0]
        self.off = {k: o for k, o in struct.iter_unpack("<II", self.d[7:7 + 8 * n])}

    def get(self, i):
        try:
            p = self.off[int(i)]
        except (KeyError, ValueError, TypeError):
            return None
        d, r, s = self.d, 0, 0
        while True:
            b = d[p]; p += 1; r |= (b & 0x7F) << s; s += 7
            if not b & 0x80:
                break
        return d[p:p + r].decode("utf-8", "replace")


def _load(n):
    f = D / f"{n}.json"
    return {int(k): v for k, v in json.loads(f.read_text(encoding="utf-8")).items()} if f.exists() else {}


items, effects, characteristics, itemtypes, spells, recipes, itemsets, monsters = _load("items"), _load("effects"), _load("characteristics"), _load("itemtypes"), _load("spells"), _load("recipes"), _load("itemsets"), _load("monsters")
npcs = _load("npcs")
breeds = _load("breeds")
_i18n = None


def i18n():
    global _i18n
    if _i18n is None:
        _i18n = I18n(D / "i18n" / f"{LANG}.bin")
    return _i18n


def text(i):
    return i18n().get(i)


def item_name(gid):
    it = items.get(gid)
    return (text(it["nameId"]) if it else None) or f"item#{gid}"


def type_name(tid):
    t = itemtypes.get(tid)
    return (text(t["nameId"]) if t else None) or f"type#{tid}"


_COND = re.compile(r"\{+~([0-9])~([0-9])([^}]*)\}+")
_PLURAL = re.compile(r"\{+~[a-z]+[^}]*\}+")


def render_effect(action, params):
    """Render an effect description. params = [p1, p2, p3...] (None = absent). Returns (text, effect_row or None)."""
    e = effects.get(action)
    tpl = text(e["descriptionId"]) if e else None
    if not tpl:
        return f"effect {action}: {params}", e
    vals = {i + 1: p for i, p in enumerate(params)}

    def cond(m):
        a, b, s = int(m.group(1)), int(m.group(2)), m.group(3)
        return s if vals.get(b) not in (None, 0) else ""
    out = _COND.sub(cond, tpl)
    out = re.sub(r"#([1-9])", lambda m: "" if vals.get(int(m.group(1))) is None else f"{vals[int(m.group(1))]:,}".replace(",", " ")
                 if isinstance(vals[int(m.group(1))], int) else str(vals[int(m.group(1))]), out)
    out = _PLURAL.sub("", out)
    out = re.sub(r"\s+", " ", re.sub(r"</?[a-z]+[^>]*>", "", out)).strip()
    return out, e


def effect_label(a):
    """Template with the numeric placeholders removed: '#1{{~1~2 à }}#2% Résistance Terre' -> '% Résistance Terre'."""
    e = effects.get(a)
    tpl = text(e["descriptionId"]) if e else None
    if not tpl:
        return None
    t = _COND.sub("", tpl)
    t = re.sub(r"#[1-9]", "", t)
    t = _PLURAL.sub("", t)
    return re.sub(r"\s+", " ", re.sub(r"<[^>]*>", "", t)).strip(" :") or None


def effect_group(a):
    e, lab = effects.get(a), effect_label(a)
    if not e or not lab or lab.startswith("-"):
        return "other"
    stat = e["useDice"] and e["characteristic"]
    if stat and "Dommage" in lab and "Résistance" not in lab and "Renvoy" not in lab:
        return "damage"
    if stat and "%" in lab and "Résistance" in lab:
        return "resist"
    return "other"


def filterable_effects():
    """Stat-like effects (those that can appear on equipment) for the UI filter."""
    out = []
    for a, e in effects.items():
        lab = effect_label(a)
        if lab and e["useDice"] and e["characteristic"]:
            out.append({"id": a, "label": lab, "group": effect_group(a)})
    return sorted(out, key=lambda x: x["label"].lower())
