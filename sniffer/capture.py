"""Start/stop-able passive capture. Emits dict records to a callback. Read-only (Npcap sniffing only)."""
import threading, time
import psutil
from scapy.all import AsyncSniffer, TCP, IP, IPv6
import pbwire, schema


class Stream:
    def __init__(self, label, direction):
        self.label, self.dir = label, direction
        self.next_seq = None
        self.pending = {}
        self.buf = bytearray()
        self.synced = False

    def feed(self, seq, data, syn=False):
        if syn:
            self.next_seq, self.synced = (seq + 1) & 0xFFFFFFFF, True
            return []
        if not data:
            return []
        if self.next_seq is None:
            self.next_seq = seq
        diff = (seq - self.next_seq) & 0xFFFFFFFF
        if diff >= 0x80000000:
            overlap = (self.next_seq - seq) & 0xFFFFFFFF
            if overlap >= len(data):
                return []
            data, seq = data[overlap:], self.next_seq
        elif diff > 0:
            self.pending[seq] = data
            return []
        self.buf += data
        self.next_seq = (seq + len(data)) & 0xFFFFFFFF
        while self.next_seq in self.pending:
            d = self.pending.pop(self.next_seq)
            self.buf += d
            self.next_seq = (self.next_seq + len(d)) & 0xFFFFFFFF
        if not self.synced:
            off = pbwire.find_sync(bytes(self.buf))
            if off is None:
                if len(self.buf) > 1 << 16:
                    self.buf.clear()
                return []
            del self.buf[:off]
            self.synced = True
        return list(pbwire.unframe(self.buf))


class Capture:
    def __init__(self, on_record, process="Dofus.exe", iface=None, on_log=print):
        self.on_record, self.process, self.iface, self.log = on_record, process, iface, on_log
        self.conns, self.streams = {}, {}
        self.lock = threading.Lock()
        self.sniffer = None
        self._stop = threading.Event()
        self.seq = 0

    @property
    def running(self):
        return self.sniffer is not None

    def start(self):
        if self.running:
            return
        self._stop.clear()
        self.streams.clear()
        self.conns.clear()
        threading.Thread(target=self._refresh, daemon=True).start()
        self.sniffer = AsyncSniffer(iface=self.iface, filter="tcp", prn=self._handle, store=False)
        self.sniffer.start()
        self.log("capture started")

    def stop(self):
        if not self.running:
            return
        self._stop.set()
        try:
            self.sniffer.stop()
        except Exception:
            pass
        self.sniffer = None
        with self.lock:
            self.conns.clear()
        self.log("capture stopped")

    def _refresh(self):
        while not self._stop.is_set():
            found = {}
            for p in psutil.process_iter(["name"]):
                if (p.info["name"] or "").lower() != self.process.lower():
                    continue
                try:
                    for c in p.net_connections(kind="tcp"):
                        if c.raddr and c.status == "ESTABLISHED":
                            found[(c.raddr.ip, c.raddr.port, c.laddr.port)] = f"{c.raddr.ip}:{c.raddr.port}"
                except psutil.Error:
                    pass
            with self.lock:
                for k, v in found.items():
                    if k not in self.conns:
                        self.log(f"tracking {v}")
                self.conns.clear()
                self.conns.update(found)
            self._stop.wait(1)

    def _handle(self, pkt):
        if TCP not in pkt:
            return
        ip = pkt[IP] if IP in pkt else pkt[IPv6] if IPv6 in pkt else None
        if ip is None:
            return
        t = pkt[TCP]
        with self.lock:
            k_out, k_in = (ip.dst, t.dport, t.sport), (ip.src, t.sport, t.dport)
            if k_out in self.conns:
                key, d = k_out, "C->S"
            elif k_in in self.conns:
                key, d = k_in, "S->C"
            else:
                return
            label = self.conns[key]
        st = self.streams.setdefault((key, d), Stream(label, d))
        for payload in st.feed(t.seq, bytes(t.payload), bool(t.flags & 0x02)):
            self._emit(label, d, payload)

    def _emit(self, label, d, payload):
        env = pbwire.parse_envelope(payload)
        self.seq += 1
        rec = {"id": self.seq, "t": round(time.time(), 3), "srv": label, "dir": d, "len": len(payload)}
        if env:
            rec.update(kind=env["kind"], uid=env["uid"], type=(env["type_url"] or "").rsplit("/", 1)[-1] or None)
            info = schema.describe(rec["type"], env["value"])
            rec["name"] = info["name"]
            rec["conf"] = info["conf"]
            rec["decoded"] = info["decoded"]
            if info.get("fmt"):
                rec["fmt"] = info["fmt"]
            if info["decoded"] is None:
                try:
                    rec["body"] = pbwire.to_obj(env["value"]) if env["value"] else {}
                except ValueError:
                    rec["body"] = "0x" + env["value"].hex()
        else:
            rec["unparsed"] = payload[:64].hex()
        rec["hex"] = payload[:4096].hex()
        self.on_record(rec)
