"""Write fixtures/ for the Rust golden tests: the synthetic AH packet (framed payload) + the python-formatted expected output."""
import json, pathlib, sys
import schema, pbwire

def _v(n):
    out = bytearray()
    while True:
        b = n & 0x7F; n >>= 7
        out.append(b | (0x80 if n else 0))
        if not n:
            return bytes(out)


def _ld(fno, b):
    return _v(fno << 3 | 2) + _v(len(b)) + b


def demo_payload():
    import game_pb2
    m = game_pb2.jzs()
    for uid, prices, effs in ((111111, [500, 4000, 0, 0], [(118, 12), (125, 40), (985, "Bob"), (2825, 0)]), (222222, [700, 0, 0, 0], [(112, None)])):
        it = m.fycg.add(); it.fyby, it.fyca, it.fycb = uid, 6, 44; it.fybz.extend(prices)
        for a, v in effs:
            e = it.fycc.add(); e.gdmq = a
            if isinstance(v, str): e.gdmv = v
            elif v is not None: e.gdmx = v
            else: e.gdmu.gdlz, e.gdmu.gdma = 2, 6
    m.fych, m.fyci = 44, 6
    any_ = _ld(1, b"type.ankama.com/jzs") + _ld(2, m.SerializeToString())
    return _ld(3, _ld(1, any_))


root = pathlib.Path(__file__).resolve().parent.parent
fx = root / "fixtures"
fx.mkdir(exist_ok=True)
payload = demo_payload()
(fx / "ah_demo.bin").write_bytes(payload)
env = pbwire.parse_envelope(payload)
info = schema.describe(env["type_url"].rsplit("/", 1)[-1], env["value"])
(fx / "ah_demo.expected.json").write_text(json.dumps({"name": info["name"], "fmt": info["fmt"]}, ensure_ascii=False, indent=1), encoding="utf-8")
print("ok", len(payload))
