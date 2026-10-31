//! Gauges that read state on each collection rather than being recorded by a
//! request. Each takes the meter to register on, and reads its state through
//! what the caller passes.

use opentelemetry::metrics::Meter;
use opentelemetry::KeyValue;
use opentelemetry_semantic_conventions::attribute::{
    DB_CLIENT_CONNECTION_POOL_NAME, DB_CLIENT_CONNECTION_STATE,
};
use opentelemetry_semantic_conventions::metric::DB_CLIENT_CONNECTION_COUNT;
use sqlx::SqlitePool;

/// `calcifer.watch.subscribers` (open `WatchEntities` streams) and
/// `calcifer.watch.revision` (the last published event's revision), read
/// through `subscribers` and `revision`.
pub fn observe_watch(
    meter: &Meter,
    subscribers: impl Fn() -> u64 + Send + Sync + 'static,
    revision: impl Fn() -> u64 + Send + Sync + 'static,
) {
    meter
        .u64_observable_gauge("calcifer.watch.subscribers")
        .with_description("Open WatchEntities streams")
        .with_unit("{stream}")
        .with_callback(move |o| o.observe(subscribers(), &[]))
        .build();
    meter
        .u64_observable_gauge("calcifer.watch.revision")
        .with_description("Revision of the last published watch event")
        .with_callback(move |o| o.observe(revision(), &[]))
        .build();
}

/// `db.client.connection.count` by state, `idle` or `used`.
pub fn observe_pool(meter: &Meter, pool: &SqlitePool) {
    let pool = pool.clone();
    meter
        .i64_observable_up_down_counter(DB_CLIENT_CONNECTION_COUNT)
        .with_description("Connections in the SQLite pool, by state")
        .with_unit("{connection}")
        .with_callback(move |o| {
            let size = i64::from(pool.size());
            let idle = i64::try_from(pool.num_idle()).unwrap_or(i64::MAX);
            let name = KeyValue::new(DB_CLIENT_CONNECTION_POOL_NAME, "calcifer");
            for (state, count) in [("idle", idle), ("used", size - idle)] {
                o.observe(
                    count,
                    &[
                        name.clone(),
                        KeyValue::new(DB_CLIENT_CONNECTION_STATE, state),
                    ],
                );
            }
        })
        .build();
}
