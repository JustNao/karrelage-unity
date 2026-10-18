//! Passive capture core. Read-only: Npcap sniffing only, nothing is injected into or read from the game.
pub mod capture;
pub mod pcap;
pub mod stream;
pub mod wire;

/// One framed protobuf message observed on a tracked connection.
#[derive(Debug, Clone)]
pub struct RawMessage {
    /// Unix time, seconds.
    pub t: f64,
    /// "ip:port" of the server.
    pub srv: String,
    /// "C->S" or "S->C".
    pub dir: &'static str,
    pub payload: Vec<u8>,
}
