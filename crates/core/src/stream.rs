//! One direction of a TCP connection: reordering, overlap trimming and frame extraction (port of capture.Stream).
use crate::wire;
use std::collections::HashMap;

/// Out-of-order segments kept while waiting for a missing one; past this the gap is given up on.
const MAX_PENDING: usize = 256;
const MAX_PENDING_BYTES: usize = 4 << 20;
/// Unsynced bytes kept while looking for a frame boundary.
const MAX_UNSYNCED: usize = 1 << 20;

#[derive(Default)]
pub struct Stream {
    next_seq: Option<u32>,
    pending: HashMap<u32, Vec<u8>>,
    pending_bytes: usize,
    buf: Vec<u8>,
    synced: bool,
}

impl Stream {
    pub fn feed(&mut self, seq: u32, data: &[u8], syn: bool) -> Vec<Vec<u8>> {
        if syn {
            self.next_seq = Some(seq.wrapping_add(1));
            self.pending.clear();
            self.pending_bytes = 0;
            self.buf.clear();
            self.synced = true;
            return vec![];
        }
        if data.is_empty() {
            return vec![];
        }
        let next = *self.next_seq.get_or_insert(seq);
        let diff = seq.wrapping_sub(next);
        let mut data = data;
        let mut seq = seq;
        if diff >= 0x8000_0000 {
            let overlap = next.wrapping_sub(seq) as usize;
            if overlap >= data.len() {
                return vec![];
            }
            data = &data[overlap..];
            seq = next;
        } else if diff > 0 {
            self.pending_bytes += data.len();
            if let Some(old) = self.pending.insert(seq, data.to_vec()) {
                self.pending_bytes -= old.len();
            }
            if self.pending.len() <= MAX_PENDING && self.pending_bytes <= MAX_PENDING_BYTES {
                return vec![];
            }
            // the missing segment is never coming (dropped by pcap, or captured mid-stream): skip the gap and
            // resync on the earliest buffered segment instead of stalling the connection for good
            let first = *self.pending.keys().min_by_key(|k| k.wrapping_sub(next)).expect("pending is not empty");
            let d = self.pending.remove(&first).unwrap_or_default();
            self.pending_bytes -= d.len();
            self.buf.clear();
            self.synced = false;
            return self.append(first, &d);
        }
        self.append(seq, data)
    }

    /// Append in-order bytes starting at `seq`, then everything pending that follows, and pop the frames.
    fn append(&mut self, seq: u32, data: &[u8]) -> Vec<Vec<u8>> {
        self.buf.extend_from_slice(data);
        let mut n = seq.wrapping_add(data.len() as u32);
        while let Some(d) = self.pending.remove(&n) {
            self.pending_bytes -= d.len();
            self.buf.extend_from_slice(&d);
            n = n.wrapping_add(d.len() as u32);
        }
        self.next_seq = Some(n);
        if !self.synced {
            match wire::find_sync(&self.buf, 3) {
                wire::SyncPoint::At(off) => {
                    self.buf.drain(..off);
                    self.synced = true;
                }
                wire::SyncPoint::Pending(drop) => {
                    self.buf.drain(..drop);
                    if self.buf.len() > MAX_UNSYNCED {
                        self.buf.clear();
                    }
                    return vec![];
                }
            }
        }
        let (frames, ok) = wire::unframe(&mut self.buf);
        if !ok {
            self.synced = false;
        }
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reorders_and_frames() {
        let mut s = Stream::default();
        assert!(s.feed(100, &[], true).is_empty());
        // frame: len 3 + payload 08 01 10 is not valid protobuf-free, but synced streams do not validate payloads.
        let a = [3u8, 8, 1, 16];
        let b = [1u8];
        assert!(s.feed(105, &b[..], false).is_empty());
        let out = s.feed(101, &a[..], false);
        assert_eq!(out, vec![vec![8, 1, 16]]);
    }

    #[test]
    fn skips_a_segment_that_never_arrives() {
        let mut s = Stream::default();
        s.feed(0, &[], true);
        // seq 1..4 is lost; everything after it must still come through once the gap is given up on
        let frame = [2u8, 0x08, 0x01];
        let mut out = vec![];
        for i in 0..(MAX_PENDING as u32 + 10) {
            out.extend(s.feed(4 + i * 3, &frame, false));
        }
        assert!(out.len() >= 5, "got {} frames", out.len());
        assert!(out.iter().all(|f| f == &vec![0x08, 0x01]));
    }
}
