"""pipeline/bindings.json -> this build's obfuscated names (what the app's datapack/bindings.json holds).

bindings.json names every field a role reads by a clear path: "Message.field.field…" = a message by its clear name (unique
in this build), then fields (or a oneof, last) by their clear names, each step but the last going into the field's message
type; a bare "Message" is the message itself. Names come from the final mapping (schema.mapping(): script table, anchors,
previous build, beta, overrides), so a role survives a game update as long as its names do. A field the beta does not
name gets its name in work/overrides.json (carried to the next build by match_protos.py)."""
import json, pathlib, sys

import builds

SRC = pathlib.Path(__file__).with_name("bindings.json")


def _messages(desc):
    sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent / "tools" / "pylib"))
    from match_protos import load
    return load(desc)[0]


def resolve(mapping, desc=builds.DESC):
    """(bindings with obfuscated names, [problems]). A role with an unresolved path is left out (the app idles it)."""
    msgs = _messages(desc)
    by_name = {}
    for k, e in mapping.items():
        if e.get("name") and k in msgs:
            by_name.setdefault(e["name"], []).append(k)

    def field(msg, clear):
        """(obf field or oneof, message type it leads to or None)"""
        names = {fk for fk, fv in mapping.get(msg, {}).get("fields", {}).items() if fv.get("name") == clear}
        m = msgs[msg]
        hits = [(f.name, f.type_name.lstrip(".") or None) for f in m.field if f.name in names] + [(o.name, None) for o in m.oneof_decl if o.name in names]
        if len(hits) != 1:
            raise LookupError(f"{clear!r} : {'introuvable' if not hits else 'ambigu'} dans {mapping.get(msg, {}).get('name') or msg}")
        return hits[0]

    def path(p):
        first, *rest = p.split(".")
        ks = by_name.get(first, [])
        if len(ks) != 1:
            raise LookupError(f"message {first!r} : {'introuvable' if not ks else f'{len(ks)} messages de ce nom'}")
        cur, obf = ks[0], ks[0]
        for i, seg in enumerate(rest):
            obf, typ = field(cur, seg)
            if i < len(rest) - 1:
                if not typ or typ not in msgs:
                    raise LookupError(f"{seg!r} n'est pas un message")
                cur = typ
        return obf

    src = json.loads(SRC.read_text(encoding="utf-8"))
    out, problems = {}, []
    for role, v in src.items():
        if role.startswith("_"):
            continue
        try:
            out[role] = {k: [path(p) for p in x] if isinstance(x, list) else path(x) for k, x in v.items() if not k.startswith("_")}
        except LookupError as e:
            problems.append(f"{role} : {e}")
    return out, problems


if __name__ == "__main__":
    import schema
    b, problems = resolve(schema.mapping())
    print(json.dumps(b, indent=1))
    for p in problems:
        print("[!]", p)
