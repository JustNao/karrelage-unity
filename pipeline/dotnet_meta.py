"""Minimal reader for the .NET (ECMA-335) metadata of a DummyDll: type definitions, nesting, fields, methods, field RVA data.

Only what script_names.py / the disassembly experiments need; no third-party package. Tables are read in their stored order,
which for Il2CppInspector's DummyDlls is the order of the types in global-metadata.dat.
"""
import struct

STR, GUID, BLOB = "str", "guid", "blob"
# coded indexes: (tag bits, tables)
CODED = {
    "TypeDefOrRef": (2, [2, 1, 27]),
    "HasConstant": (2, [4, 8, 23]),
    "HasCustomAttribute": (5, [6, 4, 1, 2, 8, 9, 10, 0, 14, 23, 20, 17, 26, 27, 32, 35, 38, 39, 40, 42, 44, 43]),
    "HasFieldMarshal": (1, [4, 8]),
    "HasDeclSecurity": (2, [2, 6, 32]),
    "MemberRefParent": (3, [2, 1, 26, 6, 27]),
    "HasSemantics": (1, [20, 23]),
    "MethodDefOrRef": (1, [6, 10]),
    "MemberForwarded": (1, [4, 6]),
    "Implementation": (2, [38, 35, 39]),
    "CustomAttributeType": (3, [None, None, 6, 10, None]),
    "ResolutionScope": (2, [0, 26, 35, 1]),
    "TypeOrMethodDef": (1, [2, 6]),
}
# row layouts; ints are struct codes, ("t", n) = index into table n
T = lambda n: ("t", n)
SCHEMA = {
    0: ["H", STR, GUID, GUID, GUID], 1: ["ResolutionScope", STR, STR], 2: ["I", STR, STR, "TypeDefOrRef", T(4), T(6)],
    3: [T(4)], 4: ["H", STR, BLOB], 5: [T(6)], 6: ["I", "H", "H", STR, BLOB, T(8)], 7: [T(8)], 8: ["H", "H", STR],
    9: [T(2), "TypeDefOrRef"], 10: ["MemberRefParent", STR, BLOB], 11: ["B", "B", "HasConstant", BLOB],
    12: ["HasCustomAttribute", "CustomAttributeType", BLOB], 13: ["HasFieldMarshal", BLOB], 14: ["H", "HasDeclSecurity", BLOB],
    15: ["H", "I", T(2)], 16: ["I", T(4)], 17: [BLOB], 18: [T(2), T(20)], 19: [T(20)], 20: ["H", STR, "TypeDefOrRef"],
    21: [T(2), T(23)], 22: [T(23)], 23: ["H", STR, BLOB], 24: ["H", T(6), "HasSemantics"], 25: [T(2), "MethodDefOrRef", "MethodDefOrRef"],
    26: [STR], 27: [BLOB], 28: ["H", "MemberForwarded", STR, T(26)], 29: ["I", T(4)], 30: ["I", "I"], 31: ["I"],
    32: ["I", "H", "H", "H", "H", "I", BLOB, STR, STR], 33: ["I"], 34: ["I", "I", "I"], 35: ["H", "H", "H", "H", "I", BLOB, STR, STR, BLOB],
    36: ["I", T(35)], 37: ["I", "I", "I", T(35)], 38: ["I", STR, BLOB], 39: ["I", "I", STR, STR, "Implementation"],
    40: ["I", "I", STR, "Implementation"], 41: [T(2), T(2)], 42: ["H", "H", "TypeOrMethodDef", STR], 43: ["MethodDefOrRef", BLOB],
    44: [T(42), "TypeDefOrRef"],
}


class Meta:
    def __init__(self, path):
        self.b = b = open(path, "rb").read()
        pe = struct.unpack_from("<I", b, 0x3C)[0]
        nsec = struct.unpack_from("<H", b, pe + 6)[0]
        opt = pe + 24
        magic = struct.unpack_from("<H", b, opt)[0]
        dd = opt + (96 if magic == 0x10B else 112)
        sec = opt + struct.unpack_from("<H", b, pe + 20)[0]
        self.sections = [struct.unpack_from("<IIII", b, sec + i * 40 + 8) for i in range(nsec)]  # vsize, va, rawsize, rawptr
        cli = self.off(struct.unpack_from("<I", b, dd + 14 * 8)[0])
        md = self.off(struct.unpack_from("<I", b, cli + 8)[0])
        vlen = struct.unpack_from("<I", b, md + 12)[0]
        p = md + 16 + vlen + 2
        streams = {}
        count = struct.unpack_from("<H", b, p)[0]; p += 2
        for _ in range(count):
            o, sz = struct.unpack_from("<II", b, p)
            name_end = b.index(b"\0", p + 8)
            streams[b[p + 8:name_end].decode()] = (md + o, sz)
            p += 8 + ((name_end - p - 8) // 4 + 1) * 4   # name is NUL-terminated, padded to 4 bytes
        self.strings = streams["#Strings"][0]
        self.blob = streams["#Blob"][0]
        t = streams["#~"][0]
        heap = b[t + 6]
        valid = struct.unpack_from("<Q", b, t + 8)[0]
        self.rows = [0] * 64
        p = t + 24
        for i in range(64):
            if valid >> i & 1:
                self.rows[i] = struct.unpack_from("<I", b, p)[0]; p += 4
        size = {STR: 4 if heap & 1 else 2, GUID: 4 if heap & 2 else 2, BLOB: 4 if heap & 4 else 2}

        def colsize(c):
            if c in size: return size[c]
            if isinstance(c, tuple): return 2 if self.rows[c[1]] < 1 << 16 else 4
            if c in CODED:
                bits, tabs = CODED[c]
                return 2 if max(self.rows[x] for x in tabs if x is not None) < 1 << (16 - bits) else 4
            return struct.calcsize("<" + c)

        self.tables = {}
        for i in range(64):
            if not self.rows[i]: continue
            cols = SCHEMA[i]
            sizes = [colsize(c) for c in cols]
            self.tables[i] = (p, cols, sizes, sum(sizes))
            p += sum(sizes) * self.rows[i]

    def off(self, rva):
        for vsize, va, rawsize, rawptr in self.sections:
            if va <= rva < va + max(vsize, rawsize):
                return rva - va + rawptr
        raise ValueError(f"rva {rva:#x} outside the sections")

    def row(self, table, i):
        """Row i (1-based) of a table, as a list of raw values (strings resolved)."""
        p, cols, sizes, rs = self.tables[table]
        p += (i - 1) * rs
        out = []
        for c, s in zip(cols, sizes):
            v = int.from_bytes(self.b[p:p + s], "little"); p += s
            out.append(self.str(v) if c == STR else v)
        return out

    def str(self, i):
        e = self.b.index(b"\0", self.strings + i)
        return self.b[self.strings + i:e].decode("utf-8", "replace")

    def blob_at(self, i):
        p = self.blob + i
        x = self.b[p]
        if x & 0x80 == 0: n, p = x, p + 1
        elif x & 0x40 == 0: n, p = (x & 0x3F) << 8 | self.b[p + 1], p + 2
        else: n, p = int.from_bytes(self.b[p:p + 4], "big") & 0x1FFFFFFF, p + 4
        return self.b[p:p + n]

    def decode_coded(self, kind, v):
        bits, tabs = CODED[kind]
        return tabs[v & ((1 << bits) - 1)], v >> bits

    def typedefs(self):
        """[{idx, name, ns, base, decl, fields: [names], methods: [(name, rva)]}] in table order."""
        n = self.rows[2]
        raw = [self.row(2, i) for i in range(1, n + 1)]
        nested = {}
        for i in range(1, self.rows[41] + 1):
            inner, outer = self.row(41, i)
            nested[inner] = outer
        nf, nm = self.rows[4], self.rows[6]
        out = []
        for i, (flags, name, ns, ext, fl, ml) in enumerate(raw, 1):
            fe = raw[i][4] if i < n else nf + 1
            me = raw[i][5] if i < n else nm + 1
            base = None
            if ext:
                tab, j = self.decode_coded("TypeDefOrRef", ext)
                base = self.row(1, j)[1] if tab == 1 else raw[j - 1][1] if tab == 2 else "?"
            out.append({"idx": i, "name": name, "ns": ns, "base": base, "decl": nested.get(i), "flags": flags,
                        "fields": [self.row(4, k)[1] for k in range(fl, fe)],
                        "methods": [(self.row(6, k)[3], self.row(6, k)[0]) for k in range(ml, me)]})
        return out

    def field_rva_data(self):
        """{field row: file offset} for fields with initial data (static array initialisers)."""
        if 29 not in self.tables: return {}
        return {f: self.off(rva) for rva, f in (self.row(29, i) for i in range(1, self.rows[29] + 1))}
