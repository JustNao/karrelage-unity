"""EXPERIMENT, not used by the pipeline: clear field names for protocol messages from the game's own code (static analysis of
GameAssembly.dll, nothing is run). Result on 3.7.12.12: 38 names for ~10k fields, half of the checkable ones wrong - the values
mostly flow through generic collections / locals, so a usable version needs real type propagation (decompiler-grade).

The game logic (Core.dll) keeps many clear field / property names (`averagePrice`, `ObjectGid`...). Wherever its compiled code
reads a field of a protocol message and stores the value into one of those, the message field gets that name:

  * Il2CppInspector's DummyDlls give every method's address ([Address(RVA)]) and every field's offset ([FieldOffset]);
  * in a protocol message class the instance fields after `_unknownFields` are the .proto fields in order (oneofs aside),
    and their property names are the obfuscated .proto field names;
  * each Core.dll method taking a protocol message is disassembled (capstone) with a small linear register tracking:
    `mov r, [msg+off]` -> r = field(msg, off); `mov [obj+off2], r` with obj = `this` / a Core-typed field -> vote for the
    name of Core field off2; a call to a Core property setter with that value -> vote for the property name.

Output: work/field_names.json {"<obf msg>": {"<obf field>": {"name", "votes", "total", "from": [...]}}}
  python pipeline/field_names.py [--report]
"""
import bisect, collections, json, pathlib, re, sys, time

root = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(root / "tools" / "pylib"))
sys.path.insert(0, str(root / "pipeline"))
from dotnet_meta import Meta
import capstone

DUMP = root / "work" / "dump" / "dll"
GA = root / "work" / "dump_input" / "GameAssembly.dll"
PROTO_DLLS = ["Ankama.Dofus.Protocol.Game.dll", "Ankama.Dofus.Protocol.Connection.dll"]
LOGIC_DLLS = ["Core.dll"]
MAX_FN = 0x4000            # bytes disassembled per method at most
ARG_REGS = ["rcx", "rdx", "r8", "r9"]
VOLATILE = {"rax", "rcx", "rdx", "r8", "r9", "r10", "r11"} | {f"xmm{i}" for i in range(6)}
R64 = {}
for b in ("ax", "bx", "cx", "dx", "si", "di", "bp", "sp"):
    for v in ("r" + b, "e" + b, b, b[0] + "l" if b[1] == "x" else b + "l"):
        R64[v] = "r" + b
for i in range(8, 16):
    for v in (f"r{i}", f"r{i}d", f"r{i}w", f"r{i}b"):
        R64[v] = f"r{i}"
for i in range(16):
    R64[f"xmm{i}"] = f"xmm{i}"
MEM = re.compile(r"^(?:\w+ ptr )?\[(\w+)(?: \+ (0x[0-9a-f]+|\d+))?\]$")


def obfuscated(name):
    return bool(re.fullmatch(r"[a-z]{2,5}", name))


def clean(name):
    """Backing field / property name -> snake_case clear name, None when it is not clear."""
    m = re.fullmatch(r"<(\w+)>k__BackingField", name)
    if m: name = m.group(1)
    name = name.lstrip("_")
    if not name or obfuscated(name) or not re.fullmatch(r"[A-Za-z][A-Za-z0-9]*", name) or name in ("value", "Value"):
        return None
    return re.sub(r"(?<=[a-z0-9])(?=[A-Z])", "_", name).lower()


class Dll:
    """Types, field offsets / types, method RVAs and signatures of one DummyDll."""

    def __init__(self, path):
        self.m = m = Meta(path)
        n = m.rows[2]
        raw = [m.row(2, i) for i in range(1, n + 1)]
        self.tname = {i: r[1] for i, r in enumerate(raw, 1)}
        self.trange = {}
        for i, r in enumerate(raw, 1):
            self.trange[i] = ((r[4], raw[i][4] if i < n else m.rows[4] + 1), (r[5], raw[i][5] if i < n else m.rows[6] + 1))
        self.foff, self.rva = {}, {}
        for i in range(1, m.rows[12] + 1):
            parent, _, blob = m.row(12, i)
            tab, j = m.decode_coded("HasCustomAttribute", parent)
            if tab not in (4, 6): continue
            b = m.blob_at(blob)
            key = b"Offset" if tab == 4 else b"RVA"
            k = b.find(key)
            if k < 0 or (tab == 4 and b.startswith(b"\x01\x00\x01\x00") and b"StaticFieldOffset" in b): continue
            mm = re.match(re.escape(key) + rb".(0x[0-9A-Fa-f]+)", b[k:], re.S)
            if mm: (self.foff if tab == 4 else self.rva)[j] = int(mm.group(1), 16)
        self.asm = {i: m.row(35, i)[6] for i in range(1, m.rows[35] + 1)}

    def field(self, k):
        flags, name, sig = self.m.row(4, k)
        return name, flags, sig

    def typeref_name(self, coded):
        """TypeDefOrRef coded index -> (assembly or None for local, type name)."""
        tab, j = self.m.decode_coded("TypeDefOrRef", coded)
        if tab == 2: return None, self.tname[j]
        if tab == 1:
            scope, name, ns = self.m.row(1, j)
            st, sj = self.m.decode_coded("ResolutionScope", scope)
            if st == 35: return self.asm.get(sj), name
            if st == 1: return self.typeref_name((sj << 2) | 1)[0], name   # nested: the enclosing ref's assembly
            return None, name
        return None, None

    def sig_types(self, blob, method=True):
        """Class types of a method's parameters [(asm, name) | None per parameter], or of a field (method=False)."""
        b = self.m.blob_at(blob)
        i = 0

        def cu():
            nonlocal i
            x = b[i]
            if x & 0x80 == 0: i += 1; return x
            if x & 0x40 == 0: v = (x & 0x3F) << 8 | b[i + 1]; i += 2; return v
            v = int.from_bytes(b[i:i + 4], "big") & 0x1FFFFFFF; i += 4; return v

        def typ():
            nonlocal i
            e = b[i]; i += 1
            if e in (0x11, 0x12): return self.typeref_name(cu())
            if e in (0x1d, 0x0f, 0x10, 0x45): typ(); return None
            if e == 0x15:
                g = typ(); n = cu()
                for _ in range(n): typ()
                return g
            if e in (0x13, 0x1e): cu(); return None
            if e in (0x1f, 0x20): cu(); return typ()
            return None
        try:
            if not method:
                i = 1; return typ()
            cc = b[i]; i += 1
            if cc & 0x10: cu()
            n = cu(); typ()
            return [typ() for _ in range(n)], bool(cc & 0x20)
        except IndexError:
            return ([], False) if method else None


def protocol_layouts():
    """{msg obf name: {offset: (field obf name, sub-message obf name or None)}} for the message classes without oneofs."""
    out = {}
    for d in PROTO_DLLS:
        p = DUMP / d
        if not p.exists(): continue
        dll = Dll(p); m = dll.m
        # property names and getter staticness, per type
        props = collections.defaultdict(list)
        pmap = [m.row(21, i) for i in range(1, m.rows[21] + 1)]
        getter = {}
        for i in range(1, m.rows[24] + 1):
            flags, meth, assoc = m.row(24, i)
            tab, pi = m.decode_coded("HasSemantics", assoc)
            if tab == 23 and flags & 2: getter[pi] = meth
        for k, (t, first) in enumerate(pmap):
            last = pmap[k + 1][1] if k + 1 < len(pmap) else m.rows[23] + 1
            for pi in range(first, last):
                g = getter.get(pi)
                gf = m.row(6, g)[2] if g is not None else 0
                # .proto fields: public instance properties (static Parser / Descriptor and the private explicit
                # IMessage.Descriptor implementation are not)
                if not gf & 0x10 and gf & 7 == 6: props[t].append(m.row(23, pi)[1])
        for t, name in dll.tname.items():
            (fs, fe), _ = dll.trange[t]
            inst = [(dll.foff[k], k) for k in range(fs, fe) if not dll.field(k)[1] & 0x10 and k in dll.foff]
            inst.sort()
            if not props.get(t) or len(inst) != len(props[t]) + 1:
                continue                                        # oneof / unexpected layout: skipped
            lay = {}
            for (off, k), pname in zip(inst[1:], props[t]):
                st = dll.sig_types(dll.field(k)[2], method=False)
                lay[off] = (pname, st[1] if st and st[0] and st[0].startswith("Ankama.Dofus.Protocol") else None)
            out[name] = lay
    return out


def main():
    t0 = time.time()
    layouts = protocol_layouts()
    print(f"protocol messages with a plain layout: {len(layouts)} ({time.time() - t0:.0f}s)")
    ga = GA.read_bytes()
    import struct
    pe = struct.unpack_from("<I", ga, 0x3C)[0]
    nsec = struct.unpack_from("<H", ga, pe + 6)[0]
    sec = pe + 24 + struct.unpack_from("<H", ga, pe + 20)[0]
    sections = [struct.unpack_from("<IIII", ga, sec + i * 40 + 8) for i in range(nsec)]

    def off(rva):
        for vs, va, rs, rp in sections:
            if va <= rva < va + rs: return rva - va + rp
        return None

    cs = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
    votes = collections.defaultdict(collections.Counter)      # (msg, field) -> Counter(clear name)
    where = collections.defaultdict(set)
    n_fn = n_ins = 0
    for d in LOGIC_DLLS:
        dll = Dll(DUMP / d); m = dll.m
        starts = sorted(set(dll.rva.values()))
        # Core field offsets / names / types, per class; Core property setters by address
        cfields = {}
        for t in dll.tname:
            (fs, fe), _ = dll.trange[t]
            fl = {}
            for k in range(fs, fe):
                name, flags, sig = dll.field(k)
                if flags & 0x10 or k not in dll.foff: continue
                st = dll.sig_types(sig, method=False)
                fl[dll.foff[k]] = (clean(name), st[1] if st and st[0] is None else None)
            cfields[dll.tname[t]] = fl
        setters = {}
        sem = {}
        for i in range(1, m.rows[24] + 1):
            flags, meth, assoc = m.row(24, i)
            tab, pi = m.decode_coded("HasSemantics", assoc)
            if tab == 23 and flags & 1: sem[meth] = m.row(23, pi)[1]
        for meth, pname in sem.items():
            if meth in dll.rva and clean(pname): setters[dll.rva[meth]] = clean(pname)
        owner = {}
        for t, (_, (ms, me)) in dll.trange.items():
            for k in range(ms, me): owner[k] = dll.tname[t]
        # clear parameter names of every Core method, by address: register slot -> name (slot 0 = rcx)
        argnames = {}
        nm, npar = m.rows[6], m.rows[8]
        for k in range(1, nm + 1):
            if k not in dll.rva: continue
            row = m.row(6, k)
            pe_ = m.row(6, k + 1)[5] if k < nm else npar + 1
            shift = 0 if row[2] & 0x10 else 1
            names = {}
            for p in range(row[5], pe_):
                flags, seq, pname = m.row(8, p)
                c = clean(pname) if pname and len(pname) >= 3 else None
                if seq >= 1 and c and seq - 1 + shift < 4: names[ARG_REGS[seq - 1 + shift]] = (c, row[3])
            if names: argnames[dll.rva[k]] = names
        for k in range(1, m.rows[6] + 1):
            if k not in dll.rva: continue
            flags, sigb = m.row(6, k)[2], m.row(6, k)[4]
            params, has_this = dll.sig_types(sigb)
            ptypes = [p[1] if p and p[0] and p[0].startswith("Ankama.Dofus.Protocol") and p[1] in layouts else None for p in params]
            if not any(ptypes): continue
            slots = ([("this", owner.get(k))] if not flags & 0x10 else []) + [("obj", p) if p else None for p in ptypes]
            state = {ARG_REGS[i]: s for i, s in enumerate(slots[:4]) if s}
            rva = dll.rva[k]
            j = bisect.bisect_right(starts, rva)
            end = min(starts[j] if j < len(starts) else rva + MAX_FN, rva + MAX_FN)
            fo = off(rva)
            if fo is None: continue
            n_fn += 1
            for addr, size, mn, ops in cs.disasm_lite(ga[fo:fo + (end - rva)], rva):
                n_ins += 1
                if mn in ("ret", "int3"): break
                a = [x.strip() for x in ops.split(",")] if ops else []
                if mn == "call":
                    if a and a[0].startswith("0x"):
                        tgt = int(a[0], 16)
                        val = state.get("rdx")
                        if tgt in setters and val and val[0] == "fld" and state.get("rcx", ("",))[0] in ("this", "core"):
                            votes[val[1:3]][setters[tgt]] += 1; where[val[1:3]].add(f"{owner.get(k)}.set_{setters[tgt]}")
                        # a message value passed as an argument whose parameter has a clear name
                        for reg, (pname, mname) in argnames.get(tgt, {}).items():
                            v = state.get(reg)
                            if v and v[0] == "fld":
                                votes[v[1:3]][pname] += 1; where[v[1:3]].add(f"{mname}({pname})")
                    for r in VOLATILE: state.pop(r, None)
                    continue
                if len(a) == 2 and mn in ("mov", "movzx", "movsx", "movsxd", "movss", "movsd", "movups", "movaps", "movdqa", "movdqu", "movq", "movd"):
                    dst, src = a
                    msrc, mdst = MEM.match(src), MEM.match(dst)
                    if dst in R64 and src in R64:
                        v = state.get(R64[src])
                        if v: state[R64[dst]] = v
                        else: state.pop(R64[dst], None)
                    elif dst in R64 and msrc:
                        base = R64.get(msrc.group(1)); disp = int(msrc.group(2) or "0", 0)
                        v = state.get(base)
                        if v and v[0] == "obj":
                            f = layouts.get(v[1], {}).get(disp)
                            state[R64[dst]] = ("fld", v[1], f[0], f[1]) if f else None
                        elif v and v[0] in ("this", "core"):
                            cf = cfields.get(v[1], {}).get(disp)
                            state[R64[dst]] = ("core", cf[1]) if cf and cf[1] else None
                        elif v and v[0] == "fld" and v[3]:
                            # the value is itself a sub-message: reading through it
                            f = layouts.get(v[3], {}).get(disp)
                            state[R64[dst]] = ("fld", v[3], f[0], f[1]) if f else None
                        else:
                            state.pop(R64[dst], None)
                        if state.get(R64[dst]) is None: state.pop(R64[dst], None)
                    elif mdst and src in R64:
                        base = R64.get(mdst.group(1)); disp = int(mdst.group(2) or "0", 0)
                        tv, sv = state.get(base), state.get(R64[src])
                        if tv and tv[0] in ("this", "core") and sv and sv[0] == "fld":
                            cf = cfields.get(tv[1], {}).get(disp)
                            if cf and cf[0]:
                                votes[sv[1:3]][cf[0]] += 1; where[sv[1:3]].add(f"{tv[1]}.{cf[0]}")
                    elif dst in R64:
                        state.pop(R64[dst], None)
                    continue
                # anything else writing its first operand register forgets what it held
                if a and a[0] in R64 and mn not in ("cmp", "test", "push") and not mn.startswith("j"):
                    state.pop(R64[a[0]], None)
    print(f"methods analysed {n_fn}, instructions {n_ins} ({time.time() - t0:.0f}s)")
    out = collections.defaultdict(dict)
    for (msg, fld), c in votes.items():
        name, n = c.most_common(1)[0]
        out[msg][fld] = {"name": name, "votes": n, "total": sum(c.values()), "from": sorted(where[(msg, fld)])[:6]}
    p = root / "work" / "field_names.json"
    p.write_text(json.dumps(out, indent=1, ensure_ascii=False), encoding="utf-8")
    print(f"named fields: {sum(len(v) for v in out.values())} in {len(out)} messages -> {p.relative_to(root)}")
    if "--report" in sys.argv:
        report(out)


def report(out):
    mp = json.loads((root / "work" / "mapping.json").read_text(encoding="utf-8"))
    by_short = {k.rsplit(".", 1)[-1]: v for k, v in mp.items()}
    agree = dis = 0; ex = []
    for msg, fs in out.items():
        mf = by_short.get(msg, {}).get("fields", {})
        for f, v in fs.items():
            b = mf.get(f, {})
            if b.get("exact"):
                if b["name"] == v["name"] or b["name"].replace("_", "") == v["name"].replace("_", ""): agree += 1
                else: dis += 1; ex.append((by_short[msg].get("name"), f, b["name"], v["name"], v["votes"], v["total"]))
    print(f"vs exact beta field names: agree {agree}, differ {dis}")
    for e in ex[:25]: print("   ", e)
    named = [(by_short.get(m, {}).get("name") or m, f, v["name"], v["votes"], v["total"]) for m, fs in out.items() for f, v in fs.items()]
    for x in named[:40]: print("  ", x)


if __name__ == "__main__":
    main()
