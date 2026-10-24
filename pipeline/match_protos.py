"""Match obfuscated current protos (work/proto/game.proto) against the clear beta protos (work/beta).

Output: work/mapping.json
  { "<obf msg>": {"name": "ChatChannelMessageEvent", "full": "...", "score": 0.93, "conf": "high|med|low",
                  "fields": {"gail": {"name": "channel", "exact": true} | {"cands": ["date","name"]}}} }

Beta field numbers are NOT used (Ankama reshuffles them each build); matching is structural:
type-multiset similarity, refined over several rounds with already-matched neighbours/enums as anchors.
Overrides in work/overrides.json ({"obf": {"name": ..., "fields": {"obfField": "clear"}}}) win over auto matches.
"""
import sys, json, pathlib, collections
root = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(root / "tools" / "pylib"))
from google.protobuf import descriptor_pb2
from grpc_tools import protoc
import grpc_tools

work = root / "work"
inc = str(pathlib.Path(grpc_tools.__file__).parent / "_proto")
F = descriptor_pb2.FieldDescriptorProto
SCALAR = {v: k for k, v in F.Type.items()}


def build_cur_desc():
    out = work / "proto" / "cur.desc"
    rc = protoc.main(["protoc", f"-I{work/'proto'}", f"-I{inc}", f"--descriptor_set_out={out}", "game.proto", "connection.proto"])
    assert rc == 0
    return out


def load(path):
    fds = descriptor_pb2.FileDescriptorSet(); fds.ParseFromString(pathlib.Path(path).read_bytes())
    msgs, enums = {}, {}

    def walk_msg(m, prefix, pkg):
        full = f"{prefix}.{m.name}" if prefix else m.name
        msgs[full] = m
        for n in m.nested_type:
            walk_msg(n, full, pkg)
        for e in m.enum_type:
            enums[f"{full}.{e.name}"] = e

    for f in fds.file:
        if f.name.startswith("google/"):
            continue
        pkg = f.package
        for m in f.message_type:
            walk_msg(m, pkg, pkg)
        for e in f.enum_type:
            enums[f"{pkg}.{e.name}" if pkg else e.name] = e
    return msgs, enums


def norm(t):
    return t.lstrip(".")


def enum_sig(e):
    return frozenset(v.number for v in e.value)


def match_enums(cur_e, beta_e):
    res = {}
    used = set()
    pairs = []
    for cn, ce in cur_e.items():
        cs = enum_sig(ce)
        for bn, be in beta_e.items():
            bs = enum_sig(be)
            j = len(cs & bs) / len(cs | bs)
            if j >= 0.6:
                pairs.append((j, -abs(len(cs) - len(bs)), cn, bn))
    pairs.sort(reverse=True)
    for j, _, cn, bn in pairs:
        if cn in res or bn in used:
            continue
        # tiny enums are too ambiguous to anchor on
        if len(enum_sig(cur_e[cn])) < 4 and j < 1:
            continue
        res[cn] = (bn, j); used.add(bn)
    return res


def main():
    cur_msgs, cur_enums = load(build_cur_desc())
    beta_msgs, beta_enums = load(work / "beta" / "beta.desc")
    # drop map-entry synthetic messages from consideration (treated as plain messages anyway)
    emap = {c: b for c, (b, j) in match_enums(cur_enums, beta_enums).items()}
    print(f"cur msgs {len(cur_msgs)}  beta msgs {len(beta_msgs)}  enums matched {len(emap)}/{len(cur_enums)}")

    return run(cur_msgs, beta_msgs, emap, script_constraints(cur_msgs, beta_msgs))


def _flat(ns):
    """Package / namespace in a comparable form: 'Com.Ankama.Dofus.Server.Game.Protocol.Tax.Collector' == '...taxcollector'."""
    return (ns or "").replace(".", "").replace("_", "").lower()


def script_constraints(cur_msgs, beta_msgs):
    """From work/script_names.json (script_names.py): {cur full name: (clear name or None, name length, package)}, plus the
    beta message each known name points to. Leaked names become fixed pairs; blank ones restrict the candidates."""
    p = work / "script_names.json"
    if not p.exists():
        return {}
    sn = json.loads(p.read_text(encoding="utf-8"))
    beta_by_short = collections.defaultdict(list)
    for bn in beta_msgs:
        beta_by_short[bn.rsplit(".", 1)[-1]].append(bn)
    out = {}
    for cn in cur_msgs:
        v = sn.get(cn.rsplit(".", 1)[-1])
        if not v:
            continue
        beta = None
        if v["name"]:
            same = [b for b in beta_by_short.get(v["name"], []) if _flat(b).startswith(_flat(v["pkg"]))] or beta_by_short.get(v["name"], [])
            beta = same[0] if len(same) == 1 else None
        out[cn] = {"name": v["name"], "len": v["len"], "pkg": v["pkg"], "beta": beta, "conf": v["conf"]}
    return out


def _ids(mapping):
    return {c: i for i, c in enumerate(mapping)}


def run(cur_msgs, beta_msgs, emap, script=None):
    benum_id = {b: i for i, b in enumerate(sorted(set(emap.values())))}
    script = script or {}
    cmap = {}   # cur -> beta
    info = {}
    # names read from the game's script table are certain: they seed the rounds as anchors
    for cn, s in script.items():
        if s["beta"] and s["beta"] not in cmap.values():
            cmap[cn] = s["beta"]; info[cn] = (1.0, -1, False)
    print(f"script table anchors: {len(cmap)}")

    def allowed(cn, bn):
        """A blank script entry still fixes the package and the length of the clear name."""
        s = script.get(cn)
        if not s or s["name"]:
            return not s
        if s["conf"] == "decoy":
            return False                          # a fake class from the obfuscator
        short = bn.rsplit(".", 1)[-1]
        return (s["len"] is None or len(short) == s["len"]) and (not s["pkg"] or _flat(bn).startswith(_flat(s["pkg"])))

    for rnd in range(7):
        pair_id = {c: i for i, c in enumerate(cmap)}
        beta_pair_id = {cmap[c]: i for c, i in pair_id.items()}

        def key_cur(f):
            card = "R" if f.label == F.LABEL_REPEATED else "S"
            if f.type == F.TYPE_ENUM:
                e = emap.get(norm(f.type_name)); return (card, "E", benum_id.get(e))
            if f.type == F.TYPE_MESSAGE:
                return (card, "M", pair_id.get(norm(f.type_name)))
            return (card, SCALAR[f.type])

        def key_beta(f):
            card = "R" if f.label == F.LABEL_REPEATED else "S"
            if f.type == F.TYPE_ENUM:
                return (card, "E", benum_id.get(norm(f.type_name)))
            if f.type == F.TYPE_MESSAGE:
                return (card, "M", beta_pair_id.get(norm(f.type_name)))
            return (card, SCALAR[f.type])

        ck = {n: collections.Counter(key_cur(f) for f in m.field) for n, m in cur_msgs.items()}
        bk = {n: collections.Counter(key_beta(f) for f in m.field) for n, m in beta_msgs.items()}
        ckL = {n: collections.Counter(k[1] for k in c.elements()) for n, c in ck.items()}
        bkL = {n: collections.Counter(k[1] for k in c.elements()) for n, c in bk.items()}
        # index beta by size for pruning
        by_size = collections.defaultdict(list)
        for n, c in bk.items():
            by_size[sum(c.values())].append(n)
        cand = []
        taken_now = pair_id_rev(cmap)
        for cn, cc in ck.items():
            if cn in cmap:
                continue
            sz = sum(cc.values())
            for s in range(max(0, sz - 4), sz + 3):
                for bn in by_size.get(s, ()):
                    if bn in taken_now or not allowed(cn, bn):
                        continue
                    bc = bk[bn]
                    inter = sum((cc & bc).values())
                    if not inter and sz:
                        continue
                    tot = (sz + sum(bc.values())) or 1
                    interL = sum((ckL[cn] & bkL[bn]).values())
                    dice = 0.4 * (2 * inter / tot) + 0.6 * (2 * interL / tot)
                    # unresolved message refs are weak evidence; penalise slightly vs concrete anchors
                    cand.append((dice, cn, bn))
        cand.sort(key=lambda x: -x[0])
        # per-cur / per-beta runner-up for ambiguity detection
        best_c, second_c = {}, {}
        best_b, second_b = {}, {}
        for d, cn, bn in cand:
            if cn not in best_c: best_c[cn] = d
            elif cn not in second_c: second_c[cn] = d
            if bn not in best_b: best_b[bn] = d
            elif bn not in second_b: second_b[bn] = d
        thr = [1.0, 0.97, 0.94, 0.9, 0.88, 0.85, 0.8][rnd]
        taken_b = set(cmap.values()); added = 0
        for d, cn, bn in cand:
            if d < thr:
                break
            if cn in cmap or bn in taken_b:
                continue
            # require uniqueness for exact-ish early rounds: top beta candidate must be unambiguous
            amb = second_c.get(cn, 0) >= d - 0.04 or second_b.get(bn, 0) >= d - 0.04
            nf = sum(ck[cn].values())
            if amb or (nf < 3 and cn not in script):   # tiny messages only with the script-table constraint
                continue
            cmap[cn] = bn; taken_b.add(bn); info[cn] = (d, rnd, amb); added += 1
        print(f"round {rnd} thr {thr}: +{added}  total {len(cmap)}")
    return cmap, info, beta_msgs, cur_msgs, emap


def pair_id_rev(cmap):
    return set(cmap.values())


def field_assign(cm, bm, cmap, emap):
    """Assign clear field names inside a matched pair by type key; ambiguous groups get candidate lists."""
    rev = {b: c for c, b in cmap.items()}

    def kc(f):
        card = "R" if f.label == F.LABEL_REPEATED else "S"
        if f.type == F.TYPE_MESSAGE:
            b = cmap.get(norm(f.type_name)); return (card, "M", b)
        if f.type == F.TYPE_ENUM:
            return (card, "E", emap.get(norm(f.type_name)))
        return (card, SCALAR[f.type])

    def kb(f):
        card = "R" if f.label == F.LABEL_REPEATED else "S"
        if f.type == F.TYPE_MESSAGE:
            return (card, "M", norm(f.type_name) if norm(f.type_name) in rev else None)
        if f.type == F.TYPE_ENUM:
            return (card, "E", norm(f.type_name) if norm(f.type_name) in emap.values() else None)
        return (card, SCALAR[f.type])

    cg, bg = collections.defaultdict(list), collections.defaultdict(list)
    for f in cm.field: cg[kc(f)].append(f)
    for f in bm.field: bg[kb(f)].append(f)
    out = {}
    unmatched_c = []
    for k, cf in cg.items():
        bf = bg.get(k, [])
        if len(cf) == 1 and len(bf) == 1:
            out[cf[0].name] = {"name": bf[0].name, "exact": True}
        elif bf:
            names = [x.name for x in bf]
            for f in cf:
                out[f.name] = {"cands": names}
        else:
            unmatched_c.extend(cf)
    # leftovers: resolve with unresolved-message keys (M,None) against remaining beta fields of same cardinality
    for f in unmatched_c:
        card = "R" if f.label == F.LABEL_REPEATED else "S"
        rest = [x.name for k, v in bg.items() if k[0] == card and k[1] == (kc(f)[1])
                for x in v if x.name not in {o.get("name") for o in out.values()}]
        if rest:
            out[f.name] = {"cands": rest, "weak": True}
    return out


# Messages the obfuscator blanked in the script table, found again on every build: the blank entry declared right after
# `after` in the same .proto file (nested decoys in between skipped), with the clear name's length, and `ints` integer
# fields (field names and numbers are reshuffled per build, so the app tells the fields apart by their values: see
# crates/modules/src/inventory.rs); `enums` = how many enum fields it has besides (default 0).
ANCHORS = [
    {"name": "ObjectMovementEvent", "file": "Inventory", "after": "ObjectDeletedEvent", "ints": (2, 3)},
    # player card tab asked for: character id + view (social / ladder / equipment); see crates/modules/src/stuff.rs
    {"name": "CardViewRequest", "file": "Card", "after": "ChangeHighlightedJobsRequest", "ints": (1, 1), "enums": 1},
]
INT_TYPES = {F.TYPE_INT32, F.TYPE_INT64, F.TYPE_UINT32, F.TYPE_UINT64, F.TYPE_SINT32, F.TYPE_SINT64}


def anchored_names(cur_msgs):
    """{cur full name: clear name} for `ANCHORS` found in work/script_names.json (declaration order)."""
    p = work / "script_names.json"
    if not p.exists():
        return {}
    entries = list(json.loads(p.read_text(encoding="utf-8")).items())
    by_short = {cn.rsplit(".", 1)[-1]: cn for cn in cur_msgs}
    out = {}
    for a in ANCHORS:
        for i, (k, v) in enumerate(entries[:-1]):
            j = i + 1
            while j < len(entries) - 1 and entries[j][1]["conf"] == "decoy":
                j += 1
            nk, nv = entries[j]
            if v["name"] != a["after"] or v["file"] != a["file"] or nv["name"] or nv["file"] != a["file"] or nv["len"] != len(a["name"]):
                continue
            cn = by_short.get(nk)
            if not cn:
                continue
            fields = cur_msgs[cn].field
            n_int = sum(f.type in INT_TYPES for f in fields)
            n_enum = sum(f.type == F.TYPE_ENUM for f in fields)
            if n_int + n_enum == len(fields) and a["ints"][0] <= n_int <= a["ints"][1] and n_enum == a.get("enums", 0):
                out[cn] = a["name"]
        if a["name"] not in out.values():
            print(f"anchor not found: {a['name']} (after {a['after']} in {a['file']})")
    return out


def message_short(full):
    return full.rsplit(".", 1)[-1]


if __name__ == "__main__":
    cmap, info, beta_msgs, cur_msgs, emap = main()
    script = script_constraints(cur_msgs, beta_msgs)
    mapping = {}
    for cn, bn in cmap.items():
        d, rnd, amb = info[cn]
        conf = "script" if rnd < 0 else "high" if d >= 0.95 and rnd <= 2 else "med" if d >= 0.8 else "low"
        mapping[cn] = {"name": message_short(bn), "full": bn, "score": round(d, 3), "round": rnd, "conf": conf,
                       "fields": field_assign(cur_msgs[cn], beta_msgs[bn], cmap, emap)}
    # script-table names with no beta counterpart (messages added since the beta): name only, no field names
    for cn, s in script.items():
        if s["name"] and cn not in mapping:
            mapping[cn] = {"name": s["name"], "full": f"{s['pkg']}.{s['name']}" if s["pkg"] else None, "conf": "script", "fields": {}}
        elif s["name"] and mapping[cn]["name"] != s["name"]:
            mapping[cn].update(name=s["name"], conf="script")
    for cn, name in anchored_names(cur_msgs).items():
        mapping.setdefault(cn, {"fields": {}}).update(name=name, conf="anchor")
    ov = work / "overrides.json"
    if ov.exists():
        for k, v in json.loads(ov.read_text()).items():
            e = mapping.setdefault(k, {"conf": "manual", "fields": {}})
            e["name"] = v.get("name", e.get("name")); e["conf"] = "manual"
            for fk, fv in v.get("fields", {}).items():
                e["fields"][fk] = {"name": fv, "exact": True}
    (work / "mapping.json").write_text(json.dumps(mapping, indent=1), encoding="utf-8")
    cnt = collections.Counter(v["conf"] for v in mapping.values())
    print("mapped", len(mapping), "of", len(cur_msgs), dict(cnt))
    for k in ("hcn", "kpy", "jnz", "jns", "itv"):
        print(k, mapping.get(k, {}).get("name"), mapping.get(k, {}).get("conf"))
