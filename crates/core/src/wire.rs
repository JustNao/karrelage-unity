//! Schemaless protobuf wire decoding + Dofus 3 framing helpers (port of pbwire.py).
use serde_json::{Map, Value};
use std::collections::HashSet;

pub struct Field<'a> {
    pub no: u64,
    pub wt: u8,
    /// Value for varint / fixed32 / fixed64 fields.
    pub num: u64,
    /// Payload of length-delimited fields.
    pub bytes: &'a [u8],
}

/// `Ok(None)` = truncated, `Err` = varint longer than 10 bytes.
pub fn read_varint(buf: &[u8], mut pos: usize) -> Result<Option<(u64, usize)>, ()> {
    let (mut result, mut shift) = (0u64, 0u32);
    loop {
        let Some(&b) = buf.get(pos) else { return Ok(None) };
        pos += 1;
        result |= ((b & 0x7F) as u64) << shift;
        if b & 0x80 == 0 {
            return Ok(Some((result, pos)));
        }
        shift += 7;
        if shift > 63 {
            return Err(());
        }
    }
}

/// Parse a message into its top-level fields. `None` if malformed.
pub fn decode_fields(data: &[u8]) -> Option<Vec<Field<'_>>> {
    let (mut out, mut pos, n) = (Vec::new(), 0usize, data.len());
    while pos < n {
        let (tag, p) = read_varint(data, pos).ok()??;
        pos = p;
        let (no, wt) = (tag >> 3, (tag & 7) as u8);
        if no == 0 {
            return None;
        }
        let mut f = Field { no, wt, num: 0, bytes: &[] };
        match wt {
            0 => {
                let (v, p) = read_varint(data, pos).ok()??;
                f.num = v;
                pos = p;
            }
            1 | 5 => {
                let len = if wt == 1 { 8 } else { 4 };
                let s = data.get(pos..pos + len)?;
                f.num = s.iter().rev().fold(0u64, |a, &b| a << 8 | b as u64);
                pos += len;
            }
            2 => {
                let (len, p) = read_varint(data, pos).ok()??;
                let end = p.checked_add(len as usize)?;
                f.bytes = data.get(p..end)?;
                pos = end;
            }
            _ => return None,
        }
        out.push(f);
    }
    Some(out)
}

pub fn looks_like_message(b: &[u8]) -> bool {
    !b.is_empty() && decode_fields(b).is_some()
}

fn is_text(b: &[u8]) -> bool {
    std::str::from_utf8(b).is_ok_and(|s| s.chars().all(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t')))
}

/// Best-effort heuristic tree `{field_no: value | [values]}` for messages without a schema.
pub fn to_obj(data: &[u8], depth: u32) -> Value {
    let mut obj = Map::new();
    let mut listy = HashSet::new();
    for f in decode_fields(data).unwrap_or_default() {
        let val = if f.wt == 2 {
            let v = f.bytes;
            if !v.is_empty() && is_text(v) && !(looks_like_message(v) && !v.is_ascii()) {
                Value::String(String::from_utf8_lossy(v).into_owned())
            } else if depth < 6 && looks_like_message(v) {
                to_obj(v, depth + 1)
            } else if is_text(v) {
                Value::String(String::from_utf8_lossy(v).into_owned())
            } else if v.len() <= 64 {
                Value::String(format!("0x{}", hex(v)))
            } else {
                Value::String(format!("<{} bytes>", v.len()))
            }
        } else {
            Value::from(f.num)
        };
        let key = f.no.to_string();
        match obj.get_mut(&key) {
            Some(Value::Array(a)) if listy.contains(&f.no) => a.push(val),
            Some(old) => {
                let first = old.take();
                *old = Value::Array(vec![first, val]);
                listy.insert(f.no);
            }
            None => {
                obj.insert(key, val);
            }
        }
    }
    Value::Object(obj)
}

pub fn hex(b: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push(DIGITS[(x >> 4) as usize] as char);
        s.push(DIGITS[(x & 15) as usize] as char);
    }
    s
}

/// Largest frame we accept; anything bigger means we are out of sync.
pub const MAX_FRAME: u64 = 16 << 20;

/// Pop complete varint-length-prefixed frames out of `buf`. The flag is `false` when the stream is out of sync
/// (the frames read before the bad length are still returned, the rest of `buf` is cleared).
pub fn unframe(buf: &mut Vec<u8>) -> (Vec<Vec<u8>>, bool) {
    let (mut out, mut at) = (Vec::new(), 0usize);
    let ok = loop {
        let Ok(r) = read_varint(buf, at) else { break false };
        let Some((len, pos)) = r else { break true };
        if len > MAX_FRAME {
            break false;
        }
        let end = pos + len as usize; // len <= MAX_FRAME here
        if end > buf.len() {
            break true;
        }
        out.push(buf[pos..end].to_vec());
        at = end;
    };
    if ok {
        buf.drain(..at);
    } else {
        buf.clear();
    }
    (out, ok)
}

/// Result of [`find_sync`].
#[derive(Debug, PartialEq, Eq)]
pub enum SyncPoint {
    /// `need` consecutive frames parse as protobuf from this offset.
    At(usize),
    /// Not found yet; no frame can start before this offset (those bytes can be dropped).
    Pending(usize),
}

/// For streams joined mid-way: offset where `need` consecutive frames all parse as protobuf. Starts whose chain is
/// cut by the end of `buf` stay undecided (they may succeed once more bytes arrive); everything before the first
/// undecided start is reported as droppable.
pub fn find_sync(buf: &[u8], need: usize) -> SyncPoint {
    let mut undecided = None;
    for start in 0..buf.len() {
        let (mut pos, mut ok, mut cut) = (start, 0, false);
        while ok < need {
            let r = read_varint(buf, pos);
            let Ok(Some((ln, p))) = r else {
                cut = matches!(r, Ok(None));
                break;
            };
            if ln == 0 || ln > MAX_FRAME {
                break;
            }
            let end = p + ln as usize;
            if end > buf.len() {
                cut = true;
                break;
            }
            if !looks_like_message(&buf[p..end]) {
                break;
            }
            pos = end;
            ok += 1;
        }
        if ok >= need {
            return SyncPoint::At(start);
        }
        if cut && undecided.is_none() {
            undecided = Some(start);
        }
    }
    SyncPoint::Pending(undecided.unwrap_or(buf.len()))
}

#[derive(Debug, Clone)]
pub struct Envelope {
    pub kind: &'static str,
    pub uid: Option<u64>,
    pub type_url: Option<String>,
    pub value: Vec<u8>,
}

/// The `type.ankama.com/<obf>` url and value of a `google.protobuf.Any`, if `b` is one.
fn any_of(b: &[u8]) -> Option<(String, Vec<u8>)> {
    let a = decode_fields(b)?;
    let url = a.iter().find(|x| x.no == 1 && x.wt == 2 && x.bytes.starts_with(b"type."))?;
    let value = a.iter().find(|x| x.no == 2 && x.wt == 2).map(|x| x.bytes.to_vec()).unwrap_or_default();
    Some((String::from_utf8_lossy(url.bytes).into_owned(), value))
}

/// Decode the GameMessage wrapper: a oneof of wrappers each carrying a `google.protobuf.Any`, with an int uid for requests
/// and responses. The wrappers' field numbers (and the Any's field in them) move between builds (3.7: client requests in
/// field 4), so they are found by shape: the first wrapper holding an Any. `kind`: "event" without uid, else "response" (a
/// client request when sent by the client: the caller knows the direction).
pub fn parse_envelope(payload: &[u8]) -> Option<Envelope> {
    for f in decode_fields(payload)? {
        if f.wt != 2 {
            continue;
        }
        let Some(inner) = decode_fields(f.bytes) else { continue };
        let Some((url, value)) = inner.iter().filter(|g| g.wt == 2).find_map(|g| any_of(g.bytes)) else { continue };
        let uid = inner.iter().find(|g| g.wt == 0).map(|g| g.num);
        let kind = if uid.is_some() { "response" } else { "event" };
        return Some(Envelope { kind, uid, type_url: Some(url), value });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_of_fixture() {
        let p = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/ah_demo.bin")).unwrap();
        let e = parse_envelope(&p).unwrap();
        assert_eq!(e.kind, "event");
        assert_eq!(e.type_url.as_deref(), Some("type.ankama.com/jzs"));
        assert!(!e.value.is_empty());
    }

    /// A client request of build 3.7 (captured): wrapper in field 4 {uid -1, Any}.
    #[test]
    fn envelope_of_client_request() {
        let p = hex_decode("222908ffffffffffffffffff01121c0a13747970652e616e6b616d612e636f6d2f6b6a74120508a298b071");
        let e = parse_envelope(&p).unwrap();
        assert_eq!((e.kind, e.type_url.as_deref()), ("response", Some("type.ankama.com/kjt")));
        assert_eq!(e.value, vec![0x08, 0xa2, 0x98, 0xb0, 0x71]);
    }

    fn hex_decode(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn find_sync_survives_garbage_lengths() {
        // 0xff.. varints decode to huge lengths; must not overflow
        let junk = [0xffu8, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x7f];
        assert!(matches!(find_sync(&junk, 3), SyncPoint::Pending(_)));
    }

    #[test]
    fn unframe_partial() {
        let mut b = vec![3, 1, 2, 3, 5, 9];
        assert_eq!(unframe(&mut b), (vec![vec![1, 2, 3]], true));
        assert_eq!(b, vec![5, 9]);
    }

    #[test]
    fn find_sync_past_a_large_partial_frame() {
        // joined inside a 10 KB frame: the next frames start far beyond the first 4 KB
        let mut b = vec![0u8; 10_000];
        for _ in 0..3 {
            b.extend_from_slice(&[2, 0x08, 0x01]);
        }
        assert_eq!(find_sync(&b, 3), SyncPoint::At(10_000));
    }
}
