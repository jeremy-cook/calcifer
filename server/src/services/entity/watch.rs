use futures::stream::BoxStream;
use sqlx::SqlitePool;
use tokio::sync::broadcast::{self, error::RecvError};
use tonic::Status;

use super::load::load_entities;
use crate::watch::WatchHub;
use calcifer::v1::{watch_entities_response, EntitySnapshot, WatchEntitiesResponse};

/// What a Watch stream sends next.
pub(super) enum WatchPhase {
    /// A snapshot: at the start, and again after the receiver lags.
    Snapshot,
    /// Events newer than the last snapshot.
    Events,
    /// Nothing: a snapshot failed to load and the error has been sent.
    Done,
}

pub(super) struct WatchState {
    pool: SqlitePool,
    hub: WatchHub,
    rx: broadcast::Receiver<WatchEntitiesResponse>,
    /// The revision the last snapshot is current as of.
    covered: u64,
    phase: WatchPhase,
}

/// The Watch stream (ADR 9): a snapshot of every entity, then each event the
/// snapshot doesn't cover, in revision order. When `rx` lags, the stream sends a
/// fresh snapshot in place of the events it missed. `rx` must already be
/// subscribed to `hub`. The first snapshot is sent as soon as the stream is
/// polled, without waiting for an event.
///
/// Why nothing is lost:
/// 1. `rx` is subscribed before the stream reads `R = hub.current_revision()`,
///    so every event with a revision above R reaches `rx` (or `rx` lags, which
///    starts over with a new snapshot).
/// 2. The snapshot is loaded after reading R. An event with a revision at or
///    below R was published before that, and writes publish only after their
///    transaction commits, so the snapshot already includes it. Skipping it
///    loses nothing.
/// 3. An event above R may repeat what the snapshot already shows, if its write
///    committed before the snapshot query ran. That's harmless: upserts and
///    deletes are idempotent, and the events after it are applied in order.
pub(super) fn watch_stream(
    pool: SqlitePool,
    hub: WatchHub,
    rx: broadcast::Receiver<WatchEntitiesResponse>,
) -> BoxStream<'static, Result<WatchEntitiesResponse, Status>> {
    let state = WatchState {
        pool,
        hub,
        rx,
        covered: 0,
        phase: WatchPhase::Snapshot,
    };
    Box::pin(futures::stream::unfold(state, |mut s| async move {
        loop {
            match s.phase {
                WatchPhase::Done => return None,
                WatchPhase::Snapshot => {
                    // Read R before loading, per step 2 above.
                    let revision = s.hub.current_revision();
                    return match load_entities(&s.pool, None).await {
                        Ok(entities) => {
                            s.covered = revision;
                            s.phase = WatchPhase::Events;
                            let event = WatchEntitiesResponse {
                                event: Some(watch_entities_response::Event::Snapshot(
                                    EntitySnapshot { entities },
                                )),
                                revision,
                            };
                            Some((Ok(event), s))
                        }
                        Err(e) => {
                            s.phase = WatchPhase::Done;
                            Some((Err(Status::from(e)), s))
                        }
                    };
                }
                WatchPhase::Events => match s.rx.recv().await {
                    Ok(event) if event.revision <= s.covered => continue,
                    Ok(event) => return Some((Ok(event), s)),
                    // Missed events: resync with the same receiver, which now
                    // holds the oldest events still in the channel.
                    Err(RecvError::Lagged(_)) => s.phase = WatchPhase::Snapshot,
                    Err(RecvError::Closed) => return None,
                },
            }
        }
    }))
}
