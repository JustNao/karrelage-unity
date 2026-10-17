"""Local web UI for the passive packet reader.  python webapp.py [--port 8765] [--on]
Serves static/index.html, streams packets over SSE, and exposes an on/off toggle for the capture."""
import argparse, collections, json, queue, pathlib, threading
from http.server import ThreadingHTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse, parse_qs
from capture import Capture
import schema
import gamedata as gd

STATIC = pathlib.Path(__file__).parent / "static"
ring = collections.deque(maxlen=5000)
clients = set()
clients_lock = threading.Lock()
log_lines = collections.deque(maxlen=50)


def broadcast(event, data):
    msg = f"event: {event}\ndata: {json.dumps(data, ensure_ascii=False, default=str)}\n\n".encode()
    with clients_lock:
        for q in list(clients):
            try:
                q.put_nowait(msg)
            except queue.Full:
                pass


STORE_FILE = pathlib.Path(__file__).resolve().parent.parent / "work" / "items_store.json"
store = json.loads(STORE_FILE.read_text(encoding="utf-8")) if STORE_FILE.exists() else {}
store_lock = threading.Lock()
import formatters
for _e in store.values():          # entries saved before 'defaults' existed
    _e.setdefault("defaults", formatters.defaults_for(_e["gid"]))


def update_store(rec):
    """Keep the latest auction listing per item (gid); a new packet for the same item replaces the old one."""
    f = rec.get("fmt")
    if not f or f.get("kind") != "auction_items" or not f.get("rows"):
        return
    gid = f.get("gid") or f["rows"][0].get("gid")
    if gid is None:
        return
    rows = f["rows"]
    mins = [r["prices"][0]["price"] for r in rows if r["prices"] and r["prices"][0]["price"] > 0]
    ent = {"gid": gid, "name": f.get("title") or rows[0]["name"], "type": f.get("type") or rows[0].get("type"),
           "level": rows[0].get("level"), "icon": rows[0].get("icon"), "updated": rec["t"], "lots": len(rows),
           "min_price": min(mins) if mins else None, "rows": rows, "defaults": f.get("defaults", []), "demo": rec.get("srv") == "demo"}
    with store_lock:
        store[str(gid)] = ent
        STORE_FILE.write_text(json.dumps({k: v for k, v in store.items() if not v.get("demo")}, ensure_ascii=False), encoding="utf-8")
    broadcast("item", ent)


def on_record(rec):
    ring.append(rec)
    broadcast("packet", rec)
    update_store(rec)


def on_log(line):
    log_lines.append(line)
    broadcast("log", line)


cap = Capture(on_record, on_log=on_log)


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


def status():
    return {"on": cap.running, "buffered": len(ring), "mapped": len(schema.mapping()),
            "tracking": sorted(cap.conns.values()), "log": list(log_lines)[-5:]}


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _json(self, obj, code=200):
        b = json.dumps(obj, ensure_ascii=False, default=str).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(b)))
        self.end_headers()
        self.wfile.write(b)

    def do_GET(self):
        u = urlparse(self.path)
        if u.path in ("/", "/index.html"):
            b = (STATIC / "index.html").read_bytes()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(b)))
            self.end_headers()
            self.wfile.write(b)
        elif u.path == "/api/status":
            self._json(status())
        elif u.path == "/api/effects":
            self._json(gd.filterable_effects())
        elif u.path == "/api/items":
            self._json(list(store.values()))
        elif u.path == "/api/packets":
            since = int(parse_qs(u.query).get("since", ["0"])[0])
            self._json([r for r in ring if r["id"] > since])
        elif u.path == "/events":
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            q = queue.Queue(maxsize=5000)
            with clients_lock:
                clients.add(q)
            try:
                self.wfile.write(b": hi\n\n")
                while True:
                    try:
                        self.wfile.write(q.get(timeout=15))
                    except queue.Empty:
                        self.wfile.write(b": ping\n\n")
                    self.wfile.flush()
            except (BrokenPipeError, ConnectionResetError, OSError):
                pass
            finally:
                with clients_lock:
                    clients.discard(q)
        else:
            self.send_error(404)

    def do_POST(self):
        n = int(self.headers.get("Content-Length") or 0)
        body = json.loads(self.rfile.read(n) or b"{}") if n else {}
        if self.path == "/api/capture":
            (cap.start if body.get("on") else cap.stop)()
            broadcast("status", status())
            self._json(status())
        elif self.path == "/api/items/clear":
            with store_lock:
                store.clear()
                STORE_FILE.write_text("{}", encoding="utf-8")
            broadcast("items_cleared", {})
            self._json(status())
        elif self.path == "/api/clear":
            ring.clear()
            self._json(status())
        elif self.path == "/api/demo":      # inject a synthetic auction-house packet to preview the formatted view
            cap._emit("demo", "S->C", demo_payload())
            self._json(status())
        elif self.path == "/api/rename":    # confirm/edit a decoded name; persisted in work/overrides.json
            schema.set_override(body.get("msg"), body.get("field"), (body.get("name") or "").strip())
            self._json({"ok": True})
        elif self.path == "/api/reload":
            schema.reload_mapping()
            self._json(status())
        else:
            self.send_error(404)


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=8765)
    ap.add_argument("--on", action="store_true", help="start capturing immediately")
    a = ap.parse_args()
    if a.on:
        cap.start()
    srv = ThreadingHTTPServer(("127.0.0.1", a.port), H)
    srv.daemon_threads = True
    print(f"http://127.0.0.1:{a.port}  (capture {'ON' if a.on else 'OFF'})", flush=True)
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        cap.stop()
