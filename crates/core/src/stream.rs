//! One direction of a TCP connection: reordering, overlap trimming and frame extraction (port of capture.Stream).
use crate::wire;
use std::collections::HashMap;

#[derive(Default)]
pub struct Stream {
    next_seq: Option<u32>,
    pending: HashMap<u32, Vec<u8>>,
    buf: Vec<u8>,
    synced: bool,
}

impl Stream {
    pub fn feed(&mut self, seq: u32, data: &[u8], syn: bool) -> Vec<Vec<u8>> {
        if syn {
            self.next_seq = Some(seq.wrapping_add(1));
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
            if self.pending.len() > 512 {
                self.pending.clear();
            }
            self.pending.insert(seq, data.to_vec());
            return vec![];
        }
        self.buf.extend_from_slice(data);
        let mut n = seq.wrapping_add(data.len() as u32);
        while let Some(d) = self.pending.remove(&n) {
            self.buf.extend_from_slice(&d);
            n = n.wrapping_add(d.len() as u32);
        }
        self.next_seq = Some(n);
        if !self.synced {
            match wire::find_sync(&self.buf, 3) {
                None => {
                    if self.buf.len() > 1 << 16 {
                        self.buf.clear();
                    }
                    return vec![];
                }
                Some(off) => {
                    self.buf.drain(..off);
                    self.synced = true;
                }
            }
        }
        match wire::unframe(&mut self.buf) {
            Ok(frames) => frames,
            Err(()) => {
                self.synced = false;
                vec![]
            }
        }
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
}
