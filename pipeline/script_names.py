"""Clear message names from the Unity script table compiled into the protocol DummyDlls (no matching involved).

Unity bakes a `MonoScriptData` table into every assembly: FilePathsData = the source files (one per .proto: Account.cs,
Exchange.cs, ...) with their number of classes, TypesData = every class as "namespace|name", file by file, in declaration
order (enums are not listed). Ankama's obfuscator blanks most entries with underscores *of the same length*, but not the
nested `Types` containers protoc generates, which carry their parent's clear full name
("Com.Ankama.Dofus.Server.Game.Protocol.Exchange.ExchangeBidHouseBuyRequest|Types"), nor some nested classes.

The classes of the DummyDll (type table order) line up one to one with those entries, file blocks in reverse order. So:
  * leaked entries name their class directly (conf "leak"), a container names its parent;
  * every blank entry still gives the class's package and the exact length of its clear name; a beta / reference / matched
    name of that package with that length confirms it (conf "len"), else the length is kept as a constraint.

Output: work/script_names.json  {"<obf>": {"name", "full", "pkg", "file", "len", "conf": "leak"|"len"|"unknown"}}
  python pipeline/script_names.py [--report]    # --report: compare with work/mapping.json and the reference names
"""
import collections, json, pathlib, re, sys

root = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(root / "pipeline"))
from dotnet_meta import Meta

DLLS = ["Ankama.Dofus.Protocol.Game.dll", "Ankama.Dofus.Protocol.Connection.dll"]
REFLECTION = "Reflection"


def be(b, i):
    return int.from_bytes(b[i:i + 4], "big")


def printable(s):
    return bool(s) and all(32 <= c <= 126 for c in s)


def script_table(b):
    """(files [(classes, file name)], types ["namespace|name" or "____"]) parsed from the raw DLL bytes.
    FilePathsData records: [int32 BE count][int32 BE len][path]; TypesData records: [byte flag][int32 BE len][text]."""
    i = b.find(b"\\Library\\PackageCache\\")
    if i < 0:
        i = b.find(b"\\Runtime\\")
        if i < 0:
            return [], []
        while i > 0 and printable(b[i - 1:i]): i -= 1
    i -= 8
    files = []
    while True:
        n, ln = be(b, i), be(b, i + 4)
        s = b[i + 8:i + 8 + ln]
        if not (0 < ln < 400 and printable(s) and s.endswith(b".cs")): break
        files.append((n, s.decode().replace("\\", "/").rsplit("/", 1)[-1][:-3])); i += 8 + ln

    def valid(t):
        ln = be(b, t + 1)
        return b[t] < 8 and 0 < ln < 400 and printable(b[t + 5:t + 5 + ln])

    def run(t, k=3):
        for _ in range(k):
            if not valid(t): return False
            t += 5 + be(b, t + 1)
        return True

    t = i
    while t < len(b) and not run(t): t += 1
    types = []
    while t < len(b) and valid(t):
        ln = be(b, t + 1); types.append(b[t + 5:t + 5 + ln].decode()); t += 5 + ln
    return files, types


def classes_of(meta):
    """Classes listed in the script table, in type-table order: reference types between the script-table holder and
    <PrivateImplementationDetails>, compiler-generated ones excluded."""
    td = meta.typedefs()
    by_idx = {t["idx"]: t for t in td}
    holder = next((t["decl"] for t in td if t["name"] == "MonoScriptData"), None)
    out, started = [], holder is None
    for t in td:
        if t["idx"] == holder:
            started = True; continue
        if t["name"].startswith("<PrivateImplementationDetails>"): break
        if not started or t["name"].startswith("<") or t["base"] in ("Enum", "ValueType") or t["decl"] == holder: continue
        out.append(t)
    return out, by_idx


def mismatches(block_entries, block_classes):
    """How many leaked entries contradict the classes they would name (nesting, protoc's field-less Types containers)."""
    bad = 0
    for e, c in zip(block_entries, block_classes):
        if "|" not in e: continue
        ns, name = e.split("|", 1)
        nested = "+" in ns or name == "Types"
        if nested != bool(c["decl"]) or (name == "Types") != (not c["fields"] and bool(c["decl"])):
            bad += 1
    return bad


def assign(cost):
    """Minimum-cost perfect assignment of a square matrix (Hungarian algorithm): column chosen for each row."""
    n = len(cost)
    INF = float("inf")
    u, v, p, way = [0] * (n + 1), [0] * (n + 1), [0] * (n + 1), [0] * (n + 1)
    for i in range(1, n + 1):
        p[0], j0 = i, 0
        minv, used = [INF] * (n + 1), [False] * (n + 1)
        while True:
            used[j0] = True
            i0, delta, j1 = p[j0], INF, 0
            for j in range(1, n + 1):
                if not used[j]:
                    cur = cost[i0 - 1][j - 1] - u[i0] - v[j]
                    if cur < minv[j]: minv[j], way[j] = cur, j0
                    if minv[j] < delta: delta, j1 = minv[j], j
            for j in range(n + 1):
                if used[j]: u[p[j]] += delta; v[j] -= delta
                else: minv[j] -= delta
            j0 = j1
            if p[j0] == 0: break
        while j0:
            j1 = way[j0]; p[j0] = p[j1]; j0 = j1
    out = [0] * n
    for j in range(1, n + 1): out[p[j] - 1] = j - 1
    return out


def name_len(e, ns):
    """Length of the clear name behind an entry, given the namespace it sits in (None when unknown)."""
    return len(e) - len(ns) - 1 if ns else None


def align(files, entries, classes, ns_of_file=None, hint=None):
    """Class for each entry. The type table holds one block per file, each starting with the file's descriptor holder
    (`<File>Reflection`, the only top-level class with a single field), in an order of its own (the protos' compile order);
    inside a block the classes come in entry order. Blocks are given to the files of the same size whose leaked entries
    fit them; ties (same size, nothing leaked) are broken with `hint` (obf -> a name found by structural matching), whose
    length must agree with the blank entries. Returns ([(file, entry, class)], files whose block stayed ambiguous)."""
    if len(entries) != len(classes) or sum(n for n, _ in files) != len(entries):
        raise SystemExit(f"script table: {len(entries)} entries, {sum(n for n, _ in files)} by file, {len(classes)} classes: layout changed")
    starts = [i for i, c in enumerate(classes) if not c["decl"] and len(c["fields"]) == 1]
    blocks = [(a, b - a) for a, b in zip(starts, starts[1:] + [len(classes)])]
    if sorted(n for _, n in blocks) != sorted(n for n, _ in files):
        raise SystemExit("script table: the class blocks do not match the files' sizes")
    e0, ent = 0, {}
    for n, f in files:
        ent[f] = entries[e0:e0 + n]; e0 += n
    def hits(f, cls):
        ns = (ns_of_file or {}).get(f)
        return sum(1 for e, c in zip(ent[f], cls) if hint and not c["decl"] and hint.get(c["name"])
                   and len(hint[c["name"]]) == name_len(e, ns))

    # blocks of one size go to the files of that size: optimal assignment (fewest contradictions, then most agreeing lengths)
    chosen, ambiguous = {}, set()
    for n in sorted({n for _, n in blocks}):
        bis = [bi for bi, (_, m) in enumerate(blocks) if m == n]
        fs = [f for m, f in files if m == n]
        cost = [[mismatches(ent[f], classes[blocks[bi][0]:blocks[bi][0] + n]) * 10000 - hits(f, classes[blocks[bi][0]:blocks[bi][0] + n])
                 for f in fs] for bi in bis]
        sol = assign(cost)
        for r, c in enumerate(sol):
            if cost[r][c] >= 10000:
                raise SystemExit(f"script table: no file fits the block at class {blocks[bis[r]][0]} ({n} classes)")
            chosen[bis[r]] = fs[c]
            # another file would have done exactly as well: the placement rests on nothing
            if any(cost[r][k] == cost[r][c] for k in range(len(fs)) if k != c) and cost[r][c] == 0:
                ambiguous.add(fs[c])
    out = []
    for bi, (a, n) in enumerate(blocks):
        f = chosen[bi]
        out += [(f, e, c) for e, c in zip(ent[f], classes[a:a + n])]
    return out, ambiguous


def leaked_namespaces(files, entries):
    """file -> package namespace, from the file's own leaked entries."""
    out, e0 = {}, 0
    for n, f in files:
        for e in entries[e0:e0 + n]:
            if "|" in e:
                out.setdefault(f, e.split("|", 1)[0].split("+")[0].rsplit(".", 1)[0])
        e0 += n
    return out


def names_for(dll, beta_ns, hint):
    meta = Meta(dll)
    files, entries = script_table(meta.b)
    if not files:
        return {}
    classes, by_idx = classes_of(meta)
    pkg_of_file = {**beta_ns, **leaked_namespaces(files, entries)}
    rows, ambiguous = align(files, entries, classes, pkg_of_file, hint)
    if ambiguous:
        print(f"  {len(ambiguous)} file(s) placed by name lengths only (same size, nothing leaked): {sorted(ambiguous)}")
    # a container ("Parent|Types") names its parent
    clear = {}                                    # class idx -> (namespace, short name)
    for f, e, c in rows:
        if "|" not in e: continue
        ns, name = e.split("|", 1)
        clear[c["idx"]] = (ns, name)
        if name == "Types" and c["decl"]:
            pns, _, pname = ns.rpartition(".") if "+" not in ns else ns.rpartition("+")
            clear.setdefault(c["decl"], (pns, pname))
    out = {}
    for f, e, c in rows:
        if c["decl"] and clear.get(c["idx"], ("", ""))[1] == "Types":
            continue                              # protoc's nested container, not a message
        pkg = pkg_of_file.get(f)
        if c["idx"] in clear:
            ns, name = clear[c["idx"]]
            conf = "leak"
        else:
            # blank: the namespace is the package (top level) or the parent's container (nested)
            if c["decl"]:
                cont = by_idx[c["decl"]]
                pn = clear.get(cont["decl"]) if cont["decl"] else None
                ns = f"{pn[0]}.{pn[1]}+Types" if pn and "+" not in pn[0] else (f"{pn[0]}+{pn[1]}+Types" if pn else None)
            else:
                ns = pkg
            name, conf = None, "unknown"
        ln = name_len(e, ns)
        if name is None and not c["decl"] and ln == len(f) + len(REFLECTION) and not c["methods"][1:] and len(c["fields"]) == 1:
            name, conf = f + REFLECTION, "leak"
        if name and name.endswith(REFLECTION) and not c["decl"]:
            continue                              # the file's descriptor holder, not a message
        if name and name.startswith("decoy"):
            name, conf = None, "decoy"            # a fake class added by the obfuscator (never sent)
        out[c["name"]] = {"name": name, "full": f"{ns}.{name}" if name and ns else None, "pkg": pkg, "file": f, "len": ln, "conf": conf,
                          "nested": bool(c["decl"]), "ambiguous": f in ambiguous}
    return out


def pascal(s):
    return "".join(w[:1].upper() + w[1:] for w in re.split(r"[_]", s))


def beta_info():
    """From the beta descriptor set (clear names): ({C# namespace: set(message names)}, {script file name: C# namespace})."""
    p = root / "work" / "beta" / "beta.desc"
    if not p.exists(): return {}, {}
    sys.path.insert(0, str(root / "tools" / "pylib"))
    from google.protobuf import descriptor_pb2
    fds = descriptor_pb2.FileDescriptorSet(); fds.ParseFromString(p.read_bytes())
    names, ns_of_file = collections.defaultdict(set), {}

    def walk(m, ns):
        names[ns].add(m.name)
        for n in m.nested_type: walk(n, ns)
    for f in fds.file:
        if f.name.startswith("google/"): continue
        ns = ".".join(pascal(x) for x in f.package.split("."))
        ns_of_file[pascal(f.name.rsplit("/", 1)[-1][:-6])] = ns
        for m in f.message_type: walk(m, ns)
    return names, ns_of_file


def main():
    dump = root / "work" / "dump" / "dll"
    beta, beta_ns = beta_info()
    ref = {}
    rp = root / "pipeline" / "reference" / "message_names.json"
    if rp.exists(): ref = json.loads(rp.read_text(encoding="utf-8"))
    mp = root / "work" / "mapping.json"
    mapping = json.loads(mp.read_text(encoding="utf-8")) if mp.exists() else {}
    hint = {o: v["name"] for o, v in mapping.items() if v.get("name")}
    hint.update(ref)
    res = {}
    for d in DLLS:
        if (dump / d).exists():
            r = names_for(dump / d, beta_ns, hint); res.update(r)
            c = collections.Counter(v["conf"] for v in r.values())
            print(f"{d}: {len(r)} messages, {dict(c)}")
    # blank names: confirm a candidate of the right package and length (reference names, current mapping)
    for obf, v in res.items():
        if v["name"] or v["len"] is None: continue
        for cand in (ref.get(obf), mapping.get(obf, {}).get("name")):
            if cand and len(cand) == v["len"] and (cand == ref.get(obf) or cand in beta.get(v["pkg"], ())):
                v["name"], v["conf"] = cand, "len"; break
    out = root / "work" / "script_names.json"
    out.write_text(json.dumps(res, indent=1, ensure_ascii=False), encoding="utf-8")
    c = collections.Counter(v["conf"] for v in res.values())
    print(f"total {len(res)}: {dict(c)} -> {out.relative_to(root)}")
    if "--report" in sys.argv:
        report(res, ref, mapping)


def report(res, ref, mapping):
    leak = {o: v for o, v in res.items() if v["conf"] == "leak"}
    agree = [o for o, n in ref.items() if o in leak and leak[o]["name"] == n]
    dis = [(o, n, leak[o]["name"]) for o, n in ref.items() if o in leak and leak[o]["name"] != n]
    print(f"reference names: {len(ref)}, also leaked {len(agree) + len(dis)}: agree {len(agree)}, disagree {len(dis)} {dis[:8]}")
    ma = md = 0; ex = []
    for o, v in leak.items():
        m = mapping.get(o, {}).get("name")
        if m:
            if m == v["name"]: ma += 1
            else: md += 1; ex.append((o, v["name"], m, mapping[o].get("conf")))
    print(f"mapping.json vs leaked: agree {ma}, disagree {md}")
    for e in ex[:15]: print("   ", e)
    lens = [(o, v) for o, v in res.items() if v["conf"] != "leak" and mapping.get(o, {}).get("name") and v["len"] is not None]
    wrong = [(o, mapping[o]["name"], v["len"]) for o, v in lens if len(mapping[o]["name"]) != v["len"]]
    print(f"blank entries with a mapping.json name: {len(lens)}, of which the name has the wrong length: {len(wrong)} {wrong[:6]}")


if __name__ == "__main__":
    main()
