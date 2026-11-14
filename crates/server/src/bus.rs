//! Every accepted change, published per game server as it happens: nobody listens yet. A later observer mode
//! (`GET /v1/stream`, a WebSocket per client) subscribes here to push changes (prices, combat information...) live.
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

/// What a subscriber receives: `{topic, rows}`.
pub type Event = Arc<Value>;

#[derive(Default)]
pub struct Bus {
    chans: Mutex<HashMap<i64, broadcast::Sender<Event>>>,
}

impl Bus {
    /// Publish to the subscribers of a game server (none: dropped at once).
    pub fn publish(&self, server: i64, event: Value) {
        if let Some(tx) = self.chans.lock().unwrap().get(&server) {
            let _ = tx.send(Arc::new(event));
        }
    }

    /// Follow a game server's changes (a slow subscriber skips the oldest ones: `RecvError::Lagged`).
    pub fn subscribe(&self, server: i64) -> broadcast::Receiver<Event> {
        self.chans.lock().unwrap().entry(server).or_insert_with(|| broadcast::channel(256).0).subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn subscribers_get_their_server_only() {
        let bus = Bus::default();
        let mut rx = bus.subscribe(295);
        bus.publish(291, json!({"topic": "prices"}));
        bus.publish(295, json!({"topic": "prices", "rows": [1]}));
        assert_eq!(*rx.try_recv().unwrap(), json!({"topic": "prices", "rows": [1]}));
        assert!(rx.try_recv().is_err());
    }
}
