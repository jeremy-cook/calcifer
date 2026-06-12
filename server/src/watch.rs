//! Live-update hub: a broadcast channel that fans EntityEvents out to every
//! `Watch` subscriber. Every successful Create/Update/Delete publishes one event;
//! open browser tabs (and the MCP client) see changes without polling.

use tokio::sync::broadcast;

use crate::proto::EntityEvent;

#[derive(Clone)]
pub struct WatchHub {
    tx: broadcast::Sender<EntityEvent>,
}

impl Default for WatchHub {
    fn default() -> Self {
        Self::new()
    }
}

impl WatchHub {
    pub fn new() -> Self {
        // Capacity 256: a slow subscriber sees RecvError::Lagged (skipped in the
        // stream wrapper) rather than backpressuring or OOMing the server.
        let (tx, _) = broadcast::channel(256);
        Self { tx }
    }

    pub fn publish(&self, event: EntityEvent) {
        // send() errors only when there are no subscribers — which is fine.
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EntityEvent> {
        self.tx.subscribe()
    }
}
