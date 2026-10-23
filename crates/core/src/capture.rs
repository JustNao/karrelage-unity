//! Start/stop-able passive capture: finds Dofus.exe's TCP connections, sniffs them, reassembles frames.
use crate::pcap::{Next, Pcap};
use crate::stream::Stream;
use crate::RawMessage;
use anyhow::Result;
use netstat2::{get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// (remote ip, remote port, local port)
type ConnKey = (IpAddr, u16, u16);

#[derive(Default)]
struct State {
    conns: HashMap<ConnKey, String>,
    streams: HashMap<(ConnKey, &'static str), Stream>,
}

/// BPF filter matching nothing (no connection tracked yet).
const NO_TRAFFIC: &str = "tcp and udp";

/// The capture filter of every adapter: only the tracked connections, so the driver drops the rest of the machine's traffic
/// before it is copied to the app. Sniff threads re-apply it when `gen` changes.
struct Filter {
    gen: AtomicU64,
    text: Mutex<String>,
}

/// BPF expression for a set of connections (remote ip + port; the local side is this machine anyway).
fn filter_of<'a>(conns: impl Iterator<Item = &'a ConnKey>) -> String {
    let mut parts: Vec<String> = conns.map(|(ip, port, _)| format!("(host {} and tcp port {port})", ip.to_canonical())).collect();
    parts.sort();
    parts.dedup();
    if parts.is_empty() {
        NO_TRAFFIC.to_string()
    } else {
        format!("tcp and ({})", parts.join(" or "))
    }
}

pub struct Capture {
    process: String,
    on_message: Arc<dyn Fn(RawMessage) + Send + Sync>,
    on_log: Arc<dyn Fn(String) + Send + Sync>,
    state: Arc<Mutex<State>>,
    filter: Arc<Filter>,
    stop: Arc<AtomicBool>,
    running: bool,
}

impl Capture {
    pub fn new(process: &str, on_message: Arc<dyn Fn(RawMessage) + Send + Sync>, on_log: Arc<dyn Fn(String) + Send + Sync>) -> Self {
        Self {
            process: process.to_lowercase(),
            on_message,
            on_log,
            state: Default::default(),
            filter: Arc::new(Filter { gen: AtomicU64::new(0), text: Mutex::new(NO_TRAFFIC.into()) }),
            stop: Arc::new(AtomicBool::new(false)),
            running: false,
        }
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn tracking(&self) -> Vec<String> {
        let mut v: Vec<String> = self.state.lock().unwrap_or_else(|e| e.into_inner()).conns.values().cloned().collect();
        v.sort();
        v
    }

    /// Feed a framed payload as if it were captured (used by the dev fixture injector).
    pub fn inject(&self, srv: &str, dir: &'static str, payload: Vec<u8>) {
        (self.on_message)(RawMessage { t: now(), srv: srv.into(), dir, payload });
    }

    pub fn start(&mut self) -> Result<()> {
        if self.running {
            return Ok(());
        }
        let api = Pcap::load()?;
        let devices = api.devices()?;
        if devices.is_empty() {
            anyhow::bail!("no usable network adapter found");
        }
        // a fresh flag per run: threads of a previous run that have not noticed their stop yet must not resume
        self.stop = Arc::new(AtomicBool::new(false));
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = State::default();
        *self.filter.text.lock().unwrap_or_else(|e| e.into_inner()) = NO_TRAFFIC.into();
        self.filter.gen.fetch_add(1, Ordering::SeqCst);

        for dev in devices {
            let (api, state, filter, stop, on_msg, on_log) =
                (api.clone(), self.state.clone(), self.filter.clone(), self.stop.clone(), self.on_message.clone(), self.on_log.clone());
            std::thread::spawn(move || match api.open(&dev.name, NO_TRAFFIC) {
                Ok(mut h) => {
                    let link = h.datalink;
                    let mut applied = 0;
                    while !stop.load(Ordering::Relaxed) {
                        let gen = filter.gen.load(Ordering::Acquire);
                        if gen != applied {
                            applied = gen;
                            let text = filter.text.lock().unwrap_or_else(|e| e.into_inner()).clone();
                            if let Err(e) = h.set_filter(&text) {
                                // never lose the game's traffic over a filter: fall back to every TCP packet
                                on_log(format!("{}: {e} (capturing all TCP)", dev.description));
                                let _ = h.set_filter("tcp");
                            }
                        }
                        match h.next() {
                            Next::Packet(p) => {
                                // a bad packet must never kill the sniff thread
                                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    if let Some(pkt) = parse_packet(p, link) {
                                        handle(&state, &on_msg, pkt);
                                    }
                                }));
                                if r.is_err() {
                                    on_log("internal error while handling a packet (ignored)".into());
                                }
                            }
                            Next::Timeout => {}
                            Next::Closed => break,
                        }
                    }
                }
                Err(e) => on_log(format!("cannot open {}: {e}", dev.description)),
            });
        }
        {
            let (state, filter, stop, name, on_log) = (self.state.clone(), self.filter.clone(), self.stop.clone(), self.process.clone(), self.on_log.clone());
            std::thread::spawn(move || refresh_loop(state, filter, stop, name, on_log));
        }
        self.running = true;
        (self.on_log)("capture started".into());
        Ok(())
    }

    pub fn stop(&mut self) {
        if !self.running {
            return;
        }
        self.stop.store(true, Ordering::SeqCst);
        self.state.lock().unwrap_or_else(|e| e.into_inner()).conns.clear();
        self.running = false;
        (self.on_log)("capture stopped".into());
    }
}

fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| (d.as_millis() as f64) / 1000.0)
}

fn refresh_loop(state: Arc<Mutex<State>>, filter: Arc<Filter>, stop: Arc<AtomicBool>, process: String, on_log: Arc<dyn Fn(String) + Send + Sync>) {
    let mut sys = sysinfo::System::new();
    while !stop.load(Ordering::Relaxed) {
        // names only: no CPU / memory / disk / exe collection for every process of the machine
        sys.refresh_processes_specifics(sysinfo::ProcessesToUpdate::All, true, sysinfo::ProcessRefreshKind::nothing());
        let pids: HashSet<u32> =
            sys.processes().iter().filter(|(_, p)| p.name().to_string_lossy().to_lowercase() == process).map(|(pid, _)| pid.as_u32()).collect();
        let mut found: HashMap<ConnKey, String> = HashMap::new();
        if !pids.is_empty() {
            if let Ok(socks) = get_sockets_info(AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6, ProtocolFlags::TCP) {
                for s in socks {
                    if let ProtocolSocketInfo::Tcp(t) = &s.protocol_socket_info {
                        if t.state == TcpState::Established && s.associated_pids.iter().any(|p| pids.contains(p)) {
                            found.insert((t.remote_addr, t.remote_port, t.local_port), format!("{}:{}", t.remote_addr, t.remote_port));
                        }
                    }
                }
            }
        }
        {
            let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
            for (k, v) in &found {
                if !st.conns.contains_key(k) {
                    on_log(format!("tracking {v}"));
                }
            }
            st.streams.retain(|(k, _), _| found.contains_key(k));
            let next = filter_of(found.keys());
            let mut text = filter.text.lock().unwrap_or_else(|e| e.into_inner());
            if *text != next {
                *text = next;
                filter.gen.fetch_add(1, Ordering::Release);
            }
            st.conns = found;
        }
        for _ in 0..10 {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

struct Pkt<'a> {
    src: IpAddr,
    dst: IpAddr,
    sport: u16,
    dport: u16,
    seq: u32,
    syn: bool,
    payload: &'a [u8],
}

fn parse_packet(p: &[u8], datalink: i32) -> Option<Pkt<'_>> {
    // DLT_EN10MB = 1 (Ethernet), DLT_NULL = 0 (loopback, 4-byte header)
    let mut off = match datalink {
        1 => 14,
        0 => 4,
        _ => return None,
    };
    let mut ethertype = if datalink == 1 { u16::from_be_bytes([*p.get(12)?, *p.get(13)?]) } else { 0x0800 };
    while ethertype == 0x8100 || ethertype == 0x88a8 {
        ethertype = u16::from_be_bytes([*p.get(off + 2)?, *p.get(off + 3)?]);
        off += 4;
    }
    let ip = p.get(off..)?;
    let (src, dst, tcp) = match ethertype {
        0x0800 => {
            let ihl = (*ip.first()? & 0x0F) as usize * 4;
            if ihl < 20 || ip.len() < 20 || ip[9] != 6 {
                return None;
            }
            let total = u16::from_be_bytes([ip[2], ip[3]]) as usize;
            let end = total.min(ip.len());
            let s: [u8; 4] = ip.get(12..16)?.try_into().ok()?;
            let d: [u8; 4] = ip.get(16..20)?.try_into().ok()?;
            (IpAddr::from(s), IpAddr::from(d), ip.get(ihl..end)?)
        }
        0x86dd => {
            if ip.len() < 40 || ip[6] != 6 {
                return None;
            }
            let plen = u16::from_be_bytes([ip[4], ip[5]]) as usize;
            let s: [u8; 16] = ip.get(8..24)?.try_into().ok()?;
            let d: [u8; 16] = ip.get(24..40)?.try_into().ok()?;
            (IpAddr::from(s), IpAddr::from(d), ip.get(40..(40 + plen).min(ip.len()))?)
        }
        _ => return None,
    };
    let doff = (*tcp.get(12)? >> 4) as usize * 4;
    if tcp.len() < 20 || doff < 20 {
        return None;
    }
    Some(Pkt {
        src,
        dst,
        sport: u16::from_be_bytes([tcp[0], tcp[1]]),
        dport: u16::from_be_bytes([tcp[2], tcp[3]]),
        seq: u32::from_be_bytes([tcp[4], tcp[5], tcp[6], tcp[7]]),
        syn: tcp[13] & 0x02 != 0,
        payload: tcp.get(doff..)?,
    })
}

fn handle(state: &Mutex<State>, on_msg: &Arc<dyn Fn(RawMessage) + Send + Sync>, p: Pkt<'_>) {
    let frames = {
        let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
        let (k_out, k_in) = ((p.dst, p.dport, p.sport), (p.src, p.sport, p.dport));
        let (key, dir) = if st.conns.contains_key(&k_out) {
            (k_out, "C->S")
        } else if st.conns.contains_key(&k_in) {
            (k_in, "S->C")
        } else {
            return;
        };
        let label = st.conns[&key].clone();
        let frames = st.streams.entry((key, dir)).or_default().feed(p.seq, p.payload, p.syn);
        frames.into_iter().map(|f| (label.clone(), dir, f)).collect::<Vec<_>>()
    };
    for (srv, dir, payload) in frames {
        on_msg(RawMessage { t: now(), srv, dir, payload });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ipv4_tcp(tcp: &[u8]) -> Vec<u8> {
        let mut p = vec![0u8; 14];
        p[12] = 0x08; // IPv4 ethertype
        let total = (20 + tcp.len()) as u16;
        let mut ip = vec![0x45, 0, (total >> 8) as u8, total as u8, 0, 0, 0, 0, 64, 6, 0, 0, 10, 0, 0, 1, 10, 0, 0, 2];
        ip.extend_from_slice(tcp);
        p.extend(ip);
        p
    }

    #[test]
    fn filter_keeps_only_tracked_connections() {
        assert_eq!(filter_of(std::iter::empty()), NO_TRAFFIC);
        let a: ConnKey = ("1.2.3.4".parse().unwrap(), 5555, 50000);
        let b: ConnKey = ("::ffff:5.6.7.8".parse().unwrap(), 443, 50001); // v4-mapped -> plain v4 (the packets are IPv4)
        let c: ConnKey = ("1.2.3.4".parse().unwrap(), 5555, 50002); // same server, other local port: one clause
        assert_eq!(filter_of([a, b, c].iter()), "tcp and ((host 1.2.3.4 and tcp port 5555) or (host 5.6.7.8 and tcp port 443))");
    }

    #[test]
    fn truncated_headers_are_ignored() {
        // 13 TCP bytes: enough for the data offset, not for the flags byte
        assert!(parse_packet(&ipv4_tcp(&[0u8; 13]), 1).is_none());
        // data offset < 20
        let mut tcp = vec![0u8; 20];
        tcp[12] = 0x20;
        assert!(parse_packet(&ipv4_tcp(&tcp), 1).is_none());
        // a valid header with a payload
        tcp[12] = 0x50;
        tcp.extend_from_slice(&[1, 2, 3]);
        assert_eq!(parse_packet(&ipv4_tcp(&tcp), 1).unwrap().payload, &[1, 2, 3]);
    }
}
