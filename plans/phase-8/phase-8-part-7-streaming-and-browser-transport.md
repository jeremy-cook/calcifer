# Phase 8 — Part 7: Streaming & browser transport

Part of `app-plan.md` Phase 8. Final server-only sub-phase. Adds the `Watch` server-streaming RPC plus the `tonic-web` and CORS layers the browser needs to talk to us in Part 8.

## Goals

1. **Watch streaming** — clients subscribe to a stream of `EntityEvent`s; every successful Create/Update/Delete publishes one. Live updates across browser tabs.
2. **gRPC-Web transport** — the browser can't speak native gRPC over fetch; `tonic-web` translates between gRPC-Web framing (browser-friendly HTTP/1.1) and native gRPC.
3. **CORS** — dev only, permissive. Hardened later (or never, if the server only ever runs on localhost).

## Why server-streaming and not bidi

Browsers' `fetch` API can stream **server → client** (the response body is a chunked `ReadableStream`) but cannot stream **client → server** in a single fetch. Connect-Web and gRPC-Web both work around this via `XMLHttpRequest`-style request body buffering. Server-streaming is the clean case; bidi/client-streaming requires WebSockets or HTTP/2 which the browser doesn't expose for fetch. We only need server-streaming for `Watch`, so we're fine.

## Files this part creates / modifies

```
server/src/watch.rs            ← NEW
server/src/services/entity.rs  ← MODIFIED (publish events; real Watch impl)
server/src/main.rs             ← MODIFIED (build hub; wrap Server with layers)
server/Cargo.toml              ← + tonic-web, tower-http, futures, tokio-stream
```

## Steps

### 1. Add deps

```toml
[dependencies]
# existing...
tonic-web = "0.12"
tower-http = { version = "0.6", features = ["cors"] }
futures = "0.3"
tokio-stream = { version = "0.1", features = ["sync"] }
```

### 2. The watch hub

`server/src/watch.rs`:

```rust
use tokio::sync::broadcast;
use crate::proto::EntityEvent;

#[derive(Clone)]
pub struct WatchHub {
    tx: broadcast::Sender<EntityEvent>,
}

impl WatchHub {
    pub fn new() -> Self {
        // Channel capacity: 256 events. Slow subscribers see Lagged errors
        // (gracefully handled in the BroadcastStream wrapper below).
        let (tx, _) = broadcast::channel(256);
        Self { tx }
    }

    pub fn publish(&self, event: EntityEvent) {
        // .send() returns Err only if there are no subscribers; we don't care.
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EntityEvent> {
        self.tx.subscribe()
    }
}
```

`tokio::sync::broadcast` is the right primitive: every subscriber sees every message (multi-consumer fan-out). Capacity-bounded so a slow subscriber can't OOM the server — it'll just see `RecvError::Lagged` and skip.

### 3. Wire the hub into `EntityService`

```rust
pub struct EntityService {
    pool: SqlitePool,
    hub: WatchHub,
}

impl EntityService {
    pub fn new(pool: SqlitePool, hub: WatchHub) -> Self {
        Self { pool, hub }
    }
}
```

After every successful `tx.commit()` in `create()`/`update()`:

```rust
let entity = self.load_entity(&entity.id).await.map_err(Status::from)?;
self.hub.publish(EntityEvent {
    event: Some(entity_event::Event::Upserted(entity.clone())),
});
Ok(Response::new(entity))
```

After `delete()`:

```rust
self.hub.publish(EntityEvent {
    event: Some(entity_event::Event::DeletedId(id)),
});
```

(`entity_event::Event` is the prost-generated oneof enum.)

### 4. `Watch` impl

```rust
use futures::Stream;
use std::pin::Pin;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

type WatchStream = Pin<Box<dyn Stream<Item = Result<EntityEvent, Status>> + Send + 'static>>;

#[tonic::async_trait]
impl entity_service_server::EntityService for EntityService {
    type WatchStream = WatchStream;

    async fn watch(&self, _: Request<WatchRequest>) -> Result<Response<Self::WatchStream>, Status> {
        let rx = self.hub.subscribe();
        let stream = BroadcastStream::new(rx)
            .filter_map(|res| async move {
                match res {
                    Ok(event) => Some(Ok(event)),
                    Err(_lagged) => None,   // skip silently
                }
            });

        Ok(Response::new(Box::pin(stream)))
    }
    // ... other methods unchanged
}
```

`Pin<Box<dyn Stream + Send>>` looks scary; it's just "a streaming object on the heap that's safe to move across `await` points." The `Pin` is required because `Stream` impls often hold self-referential state.

### 5. Build the hub in `main` and wrap with layers

```rust
mod watch;
use crate::watch::WatchHub;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let pool = db::connect(&std::env::var("DATABASE_URL")?).await?;
    let hub = WatchHub::new();

    let entity_svc = EntityServiceServer::new(EntityService::new(pool.clone(), hub.clone()));
    let richtext_svc = RichTextServiceServer::new(RichTextService::new(pool));

    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
    tracing::info!("listening on {}", addr);

    Server::builder()
        .accept_http1(true)
        .layer(tower_http::cors::CorsLayer::very_permissive())
        .layer(tonic_web::GrpcWebLayer::new())
        .add_service(entity_svc)
        .add_service(richtext_svc)
        .serve(addr)
        .await?;

    Ok(())
}
```

`accept_http1(true)` is required: gRPC-Web speaks HTTP/1.1, not HTTP/2. The default `Server` only accepts HTTP/2.

The two layers must be in this order: `CorsLayer` outermost (preflight responses bypass tonic), `GrpcWebLayer` inner.

### 6. Confirm gRPC-Web framing

Before moving to Part 8, sanity-check with a raw HTTP request:

```bash
# Encode a Get request with id="test-1":
# Prefix: 1 byte flag (0x00) + 4 bytes BE length + protobuf body.
# Easier: use grpcurl with --use-grpc-web flag if available, or rely on the
# Part 8 browser smoke test as the real verification.
```

In practice the Part 8 browser-side wire-up is the real test for this. If `cargo build` is clean and the server boots without panics, that's enough for Part 7 standalone.

## Verification

1. `cargo build` clean.
2. Server boots; logs "listening".
3. Two concurrent `grpcurl` Watch clients (over native gRPC):
   ```bash
   # Terminal A
   grpcurl -plaintext -import-path proto -proto calcifer/v1/services.proto \
     localhost:8080 calcifer.v1.EntityService/Watch
   # Terminal B (same)
   # Terminal C: trigger a Create — both A and B should see the event
   ```
4. `curl -i http://localhost:8080` returns CORS headers (`access-control-allow-origin: *` etc).
5. No panic when subscribers disconnect mid-stream.

## Known limitations

- **Slow subscribers see `Lagged`.** Capacity 256 is fine for a single user; if you ever multi-tenant, give each user their own bounded channel.
- **No reconnect/backfill.** A subscriber that disconnects and reconnects misses any events emitted while they were away. Acceptable for now since the FE refetches on reconnect anyway.
- **Permissive CORS.** Fine on localhost. Tighten if/when the server runs anywhere else.

## Done when

All 5 verification steps pass; two `grpcurl` Watch clients receive events from a third client's `Create` call.

## What's next (Part 8)

The FE side. Generate connect-es service stubs, replace Zustand `persist` with TanStack Query hooks over a gRPC-Web client, hook the Watch stream into the cache for live updates.
