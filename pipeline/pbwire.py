"""Schemaless protobuf wire decoding + Dofus 3 framing helpers (pure python, read-only)."""
from __future__ import annotations


def read_varint(buf: bytes, pos: int = 0):
    """Returns (value, new_pos) or None if truncated. Raises ValueError if >10 bytes."""
    result = shift = 0
    n = len(buf)
    while True:
        if pos >= n:
            return None
        b = buf[pos]
        pos += 1
        result |= (b & 0x7F) << shift
        if not b & 0x80:
            return result, pos
        shift += 7
        if shift > 63:
            raise ValueError("varint too long")


def decode_fields(data: bytes):
    """Parse a message into a list of (field_no, wire_type, value). Raises ValueError if malformed."""
    out, pos, n = [], 0, len(data)
    while pos < n:
        r = read_varint(data, pos)
        if r is None:
            raise ValueError("truncated tag")
        tag, pos = r
        fno, wt = tag >> 3, tag & 7
        if fno == 0:
            raise ValueError("field 0")
        if wt == 0:
            r = read_varint(data, pos)
            if r is None:
                raise ValueError("truncated varint")
            v, pos = r
        elif wt == 1:
            if pos + 8 > n:
                raise ValueError("truncated fixed64")
            v, pos = int.from_bytes(data[pos:pos + 8], "little"), pos + 8
        elif wt == 5:
            if pos + 4 > n:
                raise ValueError("truncated fixed32")
            v, pos = int.from_bytes(data[pos:pos + 4], "little"), pos + 4
        elif wt == 2:
            r = read_varint(data, pos)
            if r is None:
                raise ValueError("truncated len")
            ln, pos = r
            if pos + ln > n:
                raise ValueError("truncated bytes")
            v, pos = data[pos:pos + ln], pos + ln
        else:
            raise ValueError(f"bad wire type {wt}")
        out.append((fno, wt, v))
    return out


def looks_like_message(b: bytes) -> bool:
    if not b:
        return False
    try:
        decode_fields(b)
        return True
    except ValueError:
        return False


def _is_text(b: bytes) -> bool:
    try:
        s = b.decode("utf-8")
    except UnicodeDecodeError:
        return False
    return all(c.isprintable() or c in "\n\r\t" for c in s)


def to_obj(data: bytes, depth: int = 0):
    """Best-effort heuristic tree: {field_no: value | [values]}. Length-delimited fields are
    guessed as sub-message / text / bytes (guesses can be wrong without a schema)."""
    obj: dict = {}
    for fno, wt, v in decode_fields(data):
        if wt == 2:
            if v and _is_text(v) and not (looks_like_message(v) and not v.isascii()):
                val = v.decode()
            elif depth < 6 and looks_like_message(v):
                val = to_obj(v, depth + 1)
            elif _is_text(v):
                val = v.decode()
            else:
                val = "0x" + v.hex() if len(v) <= 64 else f"<{len(v)} bytes>"
        else:
            val = v
        if fno in obj:
            if not isinstance(obj[fno], list) or (obj[fno] and isinstance(obj[fno], list) and obj.get(("_l", fno)) is None):
                obj[fno] = [obj[fno]]
                obj[("_l", fno)] = True
            obj[fno].append(val)
        else:
            obj[fno] = val
    return {k: v for k, v in obj.items() if not isinstance(k, tuple)}


def unframe(buf: bytearray):
    """Pop complete varint-length-prefixed frames out of buf. Yields payload bytes."""
    while True:
        r = read_varint(buf, 0)
        if r is None:
            return
        ln, pos = r
        if pos + ln > len(buf):
            return
        payload = bytes(buf[pos:pos + ln])
        del buf[:pos + ln]
        yield payload


def find_sync(buf: bytes, need: int = 3):
    """For streams joined mid-way: find an offset where `need` consecutive frames all parse as protobuf."""
    for start in range(0, min(len(buf), 4096)):
        pos, ok = start, 0
        while ok < need:
            try:
                r = read_varint(buf, pos)
            except ValueError:
                break
            if r is None:
                break
            ln, p = r
            if ln == 0 or p + ln > len(buf) or not looks_like_message(buf[p:p + ln]):
                break
            pos, ok = p + ln, ok + 1
        if ok >= need:
            return start
    return None


def parse_envelope(payload: bytes):
    """Decode the GameMessage wrapper: oneof {1: Request, 2: Response, 3: Event}.
    Returns dict(kind, uid, type_url, value) or None. Field layout from forum notes; may rotate per build."""
    try:
        top = decode_fields(payload)
    except ValueError:
        return None
    for fno, wt, v in top:
        if fno in (1, 2, 3) and wt == 2:
            kind = {1: "request", 2: "response", 3: "event"}[fno]
            try:
                inner = decode_fields(v)
            except ValueError:
                continue
            uid, any_bytes = None, None
            for f2, w2, v2 in inner:
                if kind == "event" and f2 == 1 and w2 == 2:
                    any_bytes = v2
                elif kind == "request" and f2 == 2 and w2 == 2:
                    any_bytes = v2
                elif kind == "response" and f2 == 1 and w2 == 2:
                    any_bytes = v2
                elif w2 == 0 and f2 in (1, 2) and uid is None:
                    uid = v2
            if any_bytes is None:  # connection-protocol style: sub-message directly
                return dict(kind=kind, uid=uid, type_url=None, value=v)
            try:
                a = decode_fields(any_bytes)
            except ValueError:
                return dict(kind=kind, uid=uid, type_url=None, value=any_bytes)
            url = next((x.decode(errors="replace") for f, w, x in a if f == 1 and w == 2), None)
            val = next((x for f, w, x in a if f == 2 and w == 2), b"")
            return dict(kind=kind, uid=uid, type_url=url, value=val)
    return None
