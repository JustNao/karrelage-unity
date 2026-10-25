"""Human-friendly renderings of specific messages, using the extracted game data.

Field names below are the *obfuscated* ones of the current build, resolved from bindings.json's clear paths.  Ambiguous same-typed ints are resolved from the game data itself.
"""
import gamedata as gd

LOT_SIZES = [1, 10, 100, 1000]
INFO_EFFECTS = {985: "modified_by", 988: "crafted_by"}   # "Modifié par : X", "Fabriqué par : X" -> metadata, not stats
LOCK_EFFECT = 2825                                        # "Empêche les futures forgemagies" -> shield flag

# --- build-specific obfuscated names: bindings.json's clear paths resolved for this build (bindings.py) -------------
import bindings as _bindings, schema as _schema
_B, _ = _bindings.resolve(_schema.mapping())   # a role missing below = formatters disabled (schema.py)
AH_LIST = _B["ah_list"]["message"]
AH_LIST_FIELDS = dict(items=_B["ah_list"]["items"], ints=tuple(_B["ah_list"]["ints"]))
AH_ITEM_FIELDS = dict(prices=_B["ah_item"]["prices"], effects=_B["ah_item"]["effects"], ints=tuple(_B["ah_item"]["ints"]))
EFFECT_FIELDS = dict(action=tuple(_B["object_effect"]["action"]), oneof=_B["object_effect"]["oneof"])
# -----------------------------------------------------------------------------------------------------------------


def _resolve_gid_type(ints):
    """From the unordered ints (uid?, gid, type) pick (gid, type, uid): gid is an item whose typeId equals another int."""
    ints = list(ints)
    for i, g in enumerate(ints):
        it = gd.items.get(g)
        if not it:
            continue
        for j, t in enumerate(ints):
            if j != i and it["typeId"] == t:
                rest = [x for k, x in enumerate(ints) if k not in (i, j)]
                return g, t, (rest[0] if rest else None)
    # fallback: a lone int that is an item id, or an item's nameId (the server may send name ids)
    for i, g in enumerate(ints):
        if g in gd.items and g > 0 and gd.items[g]["typeId"] not in ints:
            return g, gd.items[g]["typeId"], next((x for k, x in enumerate(ints) if k != i and x not in gd.itemtypes), None)
    for i, n in enumerate(ints):
        g = _BY_NAMEID.get(n)
        if g:
            return g, gd.items[g]["typeId"], None
    return None, None, None


_BY_NAMEID = {v["nameId"]: k for k, v in gd.items.items()}


def _effect(e):
    """ObjectEffect message -> {action, text, raw}."""
    which = e.WhichOneof(EFFECT_FIELDS["oneof"])
    acts = [getattr(e, f) for f in EFFECT_FIELDS["action"]]
    action = next((a for a in acts if a in gd.effects), next((a for a in acts if a), acts[0]))
    params, raw = [], None
    if which:
        v = getattr(e, which)
        if hasattr(v, "DESCRIPTOR"):
            raw = {f.name: getattr(v, f.name) for f in v.DESCRIPTOR.fields if f.type not in (11,)}
            vals = [getattr(v, f.name) for f in v.DESCRIPTOR.fields if f.type in (3, 5, 13, 4)]
            if len(vals) == 2:                       # min/max range
                params = [vals[0], vals[1]]
            elif len(vals) == 3:                     # dice: num, side, const  (client shows min..max)
                n, s, c = vals if v.DESCRIPTOR.fields[0].type == 3 else vals
                params = [c + n, c + n * s] if n and s else [c]
        elif isinstance(v, str):
            params = [v]
            raw = v
        else:
            params, raw = [v], v
    txt, row = gd.render_effect(action, params)
    nums = [p for p in params if isinstance(p, int)]
    return {"action": action, "text": txt, "params": params, "raw": raw if isinstance(raw, (int, str, dict)) else None,
            "known": row is not None, "hi": max(nums) if nums else None, "str": raw if isinstance(raw, str) else None}


def _compare(effs, gid):
    """Annotate effects vs the item's default effects: added (not a default), above (over default max), normal; plus removed."""
    info = gd.items.get(gid)
    defaults = {}
    for d in (info or {}).get("effects", []):
        defaults.setdefault(d["a"], []).append(d)
    out, present = [], set()
    for e in effs:
        present.add(e["action"])
        d = defaults.get(e["action"])
        if info is None:
            e["status"] = "normal"
        elif not d:
            e["status"] = "added"
        elif e["hi"] is not None and e["hi"] > max(x["max"] for x in d):
            e["status"] = "above"
        else:
            e["status"] = "normal"
        if d:
            e["range"] = [min(x["min"] for x in d), max(x["max"] for x in d)]
        out.append(e)
    for a, ds in defaults.items():
        er = gd.effects.get(a)
        if a in present or a in INFO_EFFECTS or a == LOCK_EFFECT or not er or not (er["useDice"] or er["characteristic"]):
            continue
        lo, hi = min(x["min"] for x in ds), max(x["max"] for x in ds)
        txt, _ = gd.render_effect(a, [lo, hi] if hi != lo else [lo])
        out.append({"action": a, "text": txt, "params": [lo, hi], "raw": None, "known": True, "hi": hi, "status": "removed", "range": [lo, hi]})
    return out


def defaults_for(gid):
    """Consolidated default effects of an item: [{a, min, max, label}] (info/lock effects excluded)."""
    out = {}
    for d in (gd.items.get(gid) or {}).get("effects", []):
        e = out.setdefault(d["a"], {"a": d["a"], "min": d["min"], "max": d["max"]})
        e["min"], e["max"] = min(e["min"], d["min"]), max(e["max"], d["max"])
    return [dict(d, label=gd.effect_label(d["a"])) for d in out.values()
            if d["a"] not in INFO_EFFECTS and d["a"] != LOCK_EFFECT and gd.effect_label(d["a"])]


def format_ah_list(msg):
    f = AH_LIST_FIELDS
    gid0, type0, _ = _resolve_gid_type([getattr(msg, n) for n in f["ints"]])
    rows = []
    for it in getattr(msg, f["items"]):
        g = AH_ITEM_FIELDS
        gid, typ, uid = _resolve_gid_type([getattr(it, n) for n in g["ints"]])
        if gid is None:
            gid, typ, uid = gid0, type0, None
        prices = list(getattr(it, g["prices"]))
        info = gd.items.get(gid)
        rows.append({
            "uid": uid, "gid": gid, "ints": [getattr(it, n) for n in g["ints"]], "name": gd.item_name(gid), "level": info and info["level"],
            "type": gd.type_name(typ if typ is not None else info and info["typeId"]),
            "icon": info and info["iconId"],
            "prices": [{"qty": LOT_SIZES[i] if i < len(LOT_SIZES) else i, "price": p} for i, p in enumerate(prices)],
            "effects": [],
        })
        row = rows[-1]
        effs = [_effect(e) for e in getattr(it, g["effects"])]
        row["meta"] = {INFO_EFFECTS[e["action"]]: e["str"] for e in effs if e["action"] in INFO_EFFECTS}
        row["locked"] = any(e["action"] == LOCK_EFFECT for e in effs)
        row["effects"] = _compare([e for e in effs if e["action"] not in INFO_EFFECTS and e["action"] != LOCK_EFFECT], gid)
    defaults = defaults_for(gid0)
    return {"kind": "auction_items", "defaults": defaults, "title": gd.item_name(gid0) if gid0 else None,
            "type": gd.type_name(type0) if type0 else None, "gid": gid0, "rows": rows}


FORMATTERS = {AH_LIST: format_ah_list}


def format_message(type_name, msg):
    fn = FORMATTERS.get(type_name)
    if not fn:
        return None
    try:
        return fn(msg)
    except Exception as e:  # never break capture because of a formatter
        return {"kind": "error", "error": repr(e)}
