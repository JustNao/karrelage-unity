"""Write fixtures/ for the Rust golden tests: the synthetic AH packet (framed payload) + the python-formatted expected output."""
import json, pathlib, sys
sys.argv = ["x"]
import webapp, schema, pbwire

root = pathlib.Path(__file__).resolve().parent.parent
fx = root / "fixtures"
fx.mkdir(exist_ok=True)
payload = webapp.demo_payload()
(fx / "ah_demo.bin").write_bytes(payload)
env = pbwire.parse_envelope(payload)
info = schema.describe(env["type_url"].rsplit("/", 1)[-1], env["value"])
(fx / "ah_demo.expected.json").write_text(json.dumps({"name": info["name"], "fmt": info["fmt"]}, ensure_ascii=False, indent=1), encoding="utf-8")
print("ok", len(payload))
