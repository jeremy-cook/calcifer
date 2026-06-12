# Phase 8 — Part 2: Proto codegen & service skeleton

Part of `app-plan.md` Phase 8. First end-to-end RPC — but against an in-memory hardcoded value, not the DB yet.

## Goal

Wire `tonic-build` so editing `.proto` files regenerates Rust types at build time, then stub `EntityService.Get` to return a hardcoded `Entity`. By the end, `grpcurl` against `:8080` should succeed.

## Files this part creates / modifies

```
proto/calcifer/v1/services.proto       ← NEW
server/build.rs                         ← NEW
server/src/proto.rs                     ← NEW
server/src/services/mod.rs              ← NEW
server/src/services/entity.rs           ← NEW
server/src/main.rs                      ← MODIFIED (replace raw listener with tonic Server)
server/Cargo.toml                       ← + prost, tonic, tonic-build deps
```

## Steps

### 1. Author `proto/calcifer/v1/services.proto`

```proto
syntax = "proto3";
package calcifer.v1;
import "calcifer/v1/entities.proto";
import "google/protobuf/empty.proto";

service EntityService {
  rpc Get(GetEntityRequest) returns (Entity);
  rpc List(ListEntitiesRequest) returns (ListEntitiesResponse);
  rpc Create(CreateEntityRequest) returns (Entity);
  rpc Update(UpdateEntityRequest) returns (Entity);
  rpc Delete(DeleteEntityRequest) returns (google.protobuf.Empty);
  rpc Watch(WatchRequest) returns (stream EntityEvent);
}

service RichTextService {
  rpc Get(RichTextRef) returns (RichText);
  rpc Put(RichText) returns (RichText);
}

message GetEntityRequest      { string id = 1; }
message ListEntitiesRequest   { string structure_type = 1; }
message ListEntitiesResponse  { repeated Entity entities = 1; }
message CreateEntityRequest   { Entity entity = 1; }
message UpdateEntityRequest   { Entity entity = 1; }
message DeleteEntityRequest   { string id = 1; }
message WatchRequest          {}

message EntityEvent {
  oneof event {
    Entity upserted = 1;
    string deleted_id = 2;
  }
}
```

### 2. Add Rust deps

`server/Cargo.toml`:

```toml
[dependencies]
# (existing)
tokio = { version = "1", features = ["full"] }
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
# new
prost = "0.13"
prost-types = "0.13"
tonic = "0.12"

[build-dependencies]
tonic-build = "0.12"
```

### 3. `server/build.rs`

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .compile_protos(
            &[
                "../proto/calcifer/v1/entities.proto",
                "../proto/calcifer/v1/services.proto",
            ],
            &["../proto"],
        )?;
    Ok(())
}
```

Cargo runs this before compiling. Generated Rust lands in `target/debug/build/calcifer-server-*/out/calcifer.v1.rs`. **Read it** — confirms the proc-macro mental model.

### 4. `server/src/proto.rs`

```rust
// Pulls in the Rust types tonic-build generated from our .proto files.
tonic::include_proto!("calcifer.v1");
```

The `include_proto!` macro expands to `include!(concat!(env!("OUT_DIR"), "/calcifer.v1.rs"))` — i.e., it textually inlines the generated file into this module.

### 5. `server/src/services/entity.rs`

```rust
use tonic::{Request, Response, Status};

use crate::proto::{
    entity_service_server::EntityService as EntityServiceTrait,
    Entity, EntityEvent, GetEntityRequest, ListEntitiesRequest, ListEntitiesResponse,
    CreateEntityRequest, UpdateEntityRequest, DeleteEntityRequest, WatchRequest,
};

pub struct EntityService;

#[tonic::async_trait]
impl EntityServiceTrait for EntityService {
    async fn get(&self, req: Request<GetEntityRequest>) -> Result<Response<Entity>, Status> {
        let id = req.into_inner().id;
        // Hardcoded for Part 2 — DB lands in Part 3.
        let entity = Entity {
            id,
            structure_type: "Note".to_string(),
            name: "Hello from Rust".to_string(),
            properties: vec![],
            links: vec![],
            referenced_dates: vec![],
            created_at: None,
            updated_at: None,
        };
        Ok(Response::new(entity))
    }

    // The other RPCs need stubs because the trait requires them. Return Unimplemented.
    async fn list(&self, _: Request<ListEntitiesRequest>) -> Result<Response<ListEntitiesResponse>, Status> {
        Err(Status::unimplemented("list — Part 4"))
    }
    async fn create(&self, _: Request<CreateEntityRequest>) -> Result<Response<Entity>, Status> {
        Err(Status::unimplemented("create — Part 4"))
    }
    async fn update(&self, _: Request<UpdateEntityRequest>) -> Result<Response<Entity>, Status> {
        Err(Status::unimplemented("update — Part 4"))
    }
    async fn delete(&self, _: Request<DeleteEntityRequest>) -> Result<Response<()>, Status> {
        Err(Status::unimplemented("delete — Part 4"))
    }

    type WatchStream = tonic::codec::Streaming<EntityEvent>; // placeholder; rewritten in Part 7
    async fn watch(&self, _: Request<WatchRequest>) -> Result<Response<Self::WatchStream>, Status> {
        Err(Status::unimplemented("watch — Part 7"))
    }
}
```

Note the `Watch` associated type signature may need adjustment — `tonic-build` generates the trait expecting `type WatchStream: Stream<Item = Result<EntityEvent, Status>> + Send + 'static`. The placeholder above might not compile cleanly; if not, leave it as `unimplemented!()` in the body and a TODO until Part 7 — see "Compile snags" below.

### 6. `server/src/services/mod.rs`

```rust
pub mod entity;
```

### 7. Rewrite `server/src/main.rs`

```rust
use std::net::SocketAddr;
use tonic::transport::Server;

mod proto;
mod services;

use crate::proto::entity_service_server::EntityServiceServer;
use crate::services::entity::EntityService;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
    tracing::info!("listening on {}", addr);

    Server::builder()
        .add_service(EntityServiceServer::new(EntityService))
        .serve(addr)
        .await?;

    Ok(())
}
```

### 8. Smoke test

Install `grpcurl` if you don't have it (`brew install grpcurl`). Then:

```bash
grpcurl -plaintext \
  -d '{"id":"abc"}' \
  -import-path proto \
  -proto calcifer/v1/services.proto \
  localhost:8080 \
  calcifer.v1.EntityService/Get
```

Should return:

```json
{
  "id": "abc",
  "structureType": "Note",
  "name": "Hello from Rust"
}
```

## Compile snags to expect (and why they happen)

- **`Watch` associated type.** `tonic-build` generates an associated `Stream` type that's tedious to satisfy with a stub. If the placeholder above fights the compiler, the cleanest workaround is `type WatchStream = futures::stream::BoxStream<'static, Result<EntityEvent, Status>>;` and have the stub return `Err(Status::unimplemented(...))`. Add `futures = "0.3"` if needed. Part 7 replaces the type properly.
- **Field name: `name` vs `title`.** Make sure Part 0 has shipped first — the proto must already have `name`.

## Verification

1. `cargo build` succeeds. Inspect `target/debug/build/calcifer-server-*/out/calcifer.v1.rs` to see the generated trait.
2. `cargo run` boots; logs show "listening on 0.0.0.0:8080".
3. `grpcurl` Get returns the hardcoded entity (above).
4. `grpcurl` against `Create` returns `UNIMPLEMENTED` — sanity check that the trait is wired.

## Done when

All 4 verification steps pass; you've read the generated `.rs` file and can point to where the trait/structs come from.

## What's next (Part 3)

Replace the hardcoded `Get` with a real SQLite query via `sqlx::query!`. First taste of compile-time SQL checking.
