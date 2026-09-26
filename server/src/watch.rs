//! Live-update hub: a broadcast channel that fans `WatchEntitiesResponse`
//! events out to every `WatchEntities` subscriber. Every successful entity write
//! (CreateEntity, RenameEntity, SetEntityProperty, DeleteEntity, a creating
//! ResolveEntity) and every `RichTextService.PutRichText` publishes; open
//! browser tabs (and the MCP client) see changes without polling.
//!
//! The hub stamps each event with a revision, monotonic per server process.
//! `EntityService::watch_entities` uses it to line events up with the snapshot it opens
//! each stream with (ADR 9).

use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use crate::proto::{watch_entities_response, WatchEntitiesResponse};

#[derive(Clone)]
pub struct WatchHub {
    tx: broadcast::Sender<WatchEntitiesResponse>,
    /// The revision of the last published event (0 before the first). Held
    /// across increment-and-send, so channel order equals revision order.
    revision: Arc<Mutex<u64>>,
}

impl Default for WatchHub {
    fn default() -> Self {
        Self::new()
    }
}

impl WatchHub {
    pub fn new() -> Self {
        // Capacity 256: a slow subscriber sees RecvError::Lagged (answered with a
        // fresh snapshot by `watch_entities`) rather than backpressuring or OOMing the
        // server.
        Self::with_capacity(256)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            tx,
            revision: Arc::new(Mutex::new(0)),
        }
    }

    /// Publish `event` with the next revision. Call it after the write's
    /// transaction has committed: `watch_entities` relies on every event at or below a
    /// revision it read being visible to its snapshot query.
    pub fn publish(&self, event: watch_entities_response::Event) {
        let mut revision = self.revision.lock().expect("watch revision lock");
        *revision += 1;
        // send() errors only when there are no subscribers, which is fine.
        let _ = self.tx.send(WatchEntitiesResponse {
            event: Some(event),
            revision: *revision,
        });
    }

    /// The revision of the last published event (0 if none yet).
    pub fn current_revision(&self) -> u64 {
        *self.revision.lock().expect("watch revision lock")
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WatchEntitiesResponse> {
        self.tx.subscribe()
    }
}
