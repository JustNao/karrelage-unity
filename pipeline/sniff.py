"""CLI: print packets to the console and append to JSONL. Usage: python sniff.py [--out ../work/capture.jsonl]"""
import argparse, json, time
from capture import Capture

ap = argparse.ArgumentParser()
ap.add_argument("--out", default="../work/capture.jsonl")
ap.add_argument("--iface")
a = ap.parse_args()
f = open(a.out, "a", encoding="utf-8")


def on_rec(r):
    f.write(json.dumps(r, ensure_ascii=False, default=str) + "\n")
    f.flush()
    body = json.dumps(r.get("decoded") or r.get("body") or r.get("unparsed"), ensure_ascii=False, default=str)
    print(f"{r['dir']} {r.get('kind', '?'):8} {r.get('name') or r.get('type')} {body[:200]}", flush=True)


c = Capture(on_rec, iface=a.iface)
c.start()
try:
    while True:
        time.sleep(1)
except KeyboardInterrupt:
    c.stop()
