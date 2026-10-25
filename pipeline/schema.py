"""Schema-aware decoding using protos generated from the local IL2CPP dump (work/pb),
with clear names applied from work/mapping.json (see match_protos.py)."""
import sys, json, pathlib

root = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(root / "tools" / "pylib"))   # protobuf runtime matching generated code
sys.path.insert(0, str(root / "work" / "pb"))
try:
    import game_pb2, connection_pb2  # noqa: F401
    from google.protobuf import descriptor_pool, message_factory, json_format
    from google.protobuf.descriptor import FieldDescriptor as FD
    _pool = descriptor_pool.Default()
    AVAILABLE = True
except Exception as e:  # pragma: no cover
    print(f"[!] schema decoding disabled: {e}", file=sys.stderr)
    AVAILABLE = False

_cache = {}
_mapping = None
_mpath = root / "work" / "mapping.json"


_opath = root / "work" / "overrides.json"
_rpath = pathlib.Path(__file__).with_name("reference") / "message_names.json"   # obf type -> clear name (import_reference_names.py)
_spath = root / "work" / "script_names.json"   # names / name lengths from the game's script table (script_names.py)


def overrides():
    return json.loads(_opath.read_text(encoding="utf-8")) if _opath.exists() else {}


def mapping():
    """work/mapping.json (auto beta matches + names read from the game's script table, conf "script"), then the reference
    name list, then work/overrides.json (confirmed by hand) on top. A reference name never replaces a script-table name, nor
    contradicts the name length the script table gives (work/script_names.json), nor an anchor / previous-build name."""
    global _mapping
    if _mapping is None:
        m = json.loads(_mpath.read_text(encoding="utf-8")) if _mpath.exists() else {}
        sn = json.loads(_spath.read_text(encoding="utf-8")) if _spath.exists() else {}
        for obf, name in (json.loads(_rpath.read_text(encoding="utf-8")) if _rpath.exists() else {}).items():
            s = sn.get(obf, {})
            if m.get(obf, {}).get("conf") in ("script", "anchor", "previous") or (s.get("len") is not None and s["len"] != len(name)):
                continue
            e = m.setdefault(obf, {"fields": {}})
            e["name"], e["conf"] = name, "ref"
        for k, v in overrides().items():
            if k.startswith("_"):   # "_build": the build the obfuscated keys belong to (match_protos.py migrate_overrides)
                continue
            e = m.setdefault(k, {"fields": {}})
            e.setdefault("fields", {})
            if v.get("name"):
                e["name"] = v["name"]
                e["conf"] = "manual"
            for fk, fv in v.get("fields", {}).items():
                e["fields"][fk] = {"name": fv, "exact": True, "manual": True}
        _mapping = m
    return _mapping


def set_override(msg, field, name):
    """Persist a confirmed name (field=None -> the message's own name). Empty name removes the override."""
    ov = overrides()
    e = ov.setdefault(msg, {})
    if field:
        e.setdefault("fields", {})
        if name:
            e["fields"][field] = name
        else:
            e["fields"].pop(field, None)
    elif name:
        e["name"] = name
    else:
        e.pop("name", None)
    if not e.get("fields"):
        e.pop("fields", None)
    if not e:
        ov.pop(msg)
    tmp = _opath.with_suffix(".tmp")
    tmp.write_text(json.dumps(ov, indent=1, ensure_ascii=False), encoding="utf-8")
    tmp.replace(_opath)
    reload_mapping()


def reload_mapping():
    global _mapping
    _mapping = None


def _field_label(msg_name, fname):
    f = mapping().get(msg_name, {}).get("fields", {}).get(fname)
    if not f:
        return fname
    if "name" in f:
        return f["name"]
    taken = {x.get("name") for x in mapping().get(msg_name, {}).get("fields", {}).values() if x.get("name")}
    c = [x for x in (f.get("cands") or []) if x not in taken]   # drop candidates already confirmed on another field
    return f"{fname} ({'|'.join(c)}?)" if c else fname


def _rename(msg, d):
    """Rename keys of MessageToDict output using the beta mapping; recurses through sub-messages."""
    out = {}
    mname = msg.DESCRIPTOR.full_name
    meta = {}
    for fd in msg.DESCRIPTOR.fields:
        if fd.name not in d:
            continue
        v = d[fd.name]
        if fd.type == FD.TYPE_MESSAGE and not fd.message_type.GetOptions().map_entry:
            val = getattr(msg, fd.name)
            if fd.is_repeated:
                v = [_rename(x, vv) for x, vv in zip(val, v)]
            else:
                v = _rename(val, v)
        lab = _field_label(mname, fd.name)
        out[lab] = v
        meta[lab] = fd.name
    out["__t"] = mname      # message (descriptor) name + label->obfuscated-field map, used by the UI editor
    out["__f"] = meta
    return out


def describe(type_name, value):
    """-> {name, conf, decoded}. name = clear name if matched, else the obfuscated type name."""
    res = {"name": type_name, "conf": None, "decoded": None, "fmt": None}
    if not AVAILABLE or not type_name:
        return res
    try:
        cls = _cache.get(type_name)
        if cls is None:
            try:
                cls = message_factory.GetMessageClass(_pool.FindMessageTypeByName(type_name))
            except KeyError:
                cls = False
            _cache[type_name] = cls
        if not cls:
            return res
        m = cls()
        m.ParseFromString(value)
        d = json_format.MessageToDict(m, preserving_proto_field_name=True)
        res["decoded"] = _rename(m, d)
        res["fmt"] = formatters.format_message(type_name, m)
        mp = mapping().get(type_name)
        if mp and mp.get("name"):
            res["name"] = mp["name"]
            res["conf"] = mp.get("conf")
    except Exception as e:
        res["decoded"] = {"_error": str(e)}
    return res


# last: formatters reads mapping() (bindings.py) when imported
try:
    import formatters
except Exception as _e:  # game data not extracted yet
    print(f"[!] formatters disabled: {_e}", file=sys.stderr)
    formatters = type("F", (), {"format_message": staticmethod(lambda *a: None)})
