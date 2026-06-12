# Phase 8 — Part 1: Workspace & hello server

Part of `app-plan.md` Phase 8. **First Rust file lands here.** No proto, no DB, no RPC yet — just a binary that boots, listens, and exits cleanly.

## Goal

Stand up the Rust workspace under `server/` and run a process listening on `:8080`. The point is to validate the toolchain and the dev loop before we add complexity.

## What exists today

- `proto/calcifer/v1/entities.proto` — the existing schema (Phase 3)
- `calcifer/` — the Vite app, currently using localStorage
- Nothing under `server/`

## Files this part creates

```
server/
  Cargo.toml
  .gitignore             ← optional; or roll into root .gitignore
  src/
    main.rs
```

Plus an entry in a root `.gitignore` for `server/target/`.

## Steps

### 1. Cargo init

From the repo root:

```bash
cargo new --bin server
```

This creates `server/Cargo.toml` and `server/src/main.rs` with a hello-world. `cargo new` initializes a git repo inside `server/` — delete that nested `.git` so we stay in the calcifer repo.

```bash
rm -rf server/.git
```

### 2. Gitignore

Create `/Users/bebop/Github/calcifer/.gitignore` at the repo root (none exists yet) with:

```
server/target/
book/book/
```

(Adding `book/book/` early since Part 0 of Track B will land soon.)

### 3. Dependencies

Edit `server/Cargo.toml`:

```toml
[package]
name = "calcifer-server"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
```

The `full` feature on tokio is wide but appropriate during scaffolding — we'll narrow later if `cargo bloat` complains.

### 4. `src/main.rs`

```rust
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("listening on {}", addr);

    // Placeholder: accept-and-drop loop until tonic lands in Part 2.
    loop {
        let (_socket, peer) = listener.accept().await?;
        tracing::debug!("accepted from {}", peer);
    }
}
```

This is intentionally a "raw TCP" listener — Part 2 replaces the loop with `tonic`'s `Server::builder()`. We want to see the runtime/tracing/listener pieces work in isolation first.

### 5. Dev loop

```bash
cargo install cargo-watch     # one-time, global
```

Document in `server/README.md` (or as comment in `Cargo.toml`):
```bash
cargo watch -x run
```

This is your equivalent of `pnpm dev` — file save → recompile → relaunch.

## Verification

1. From `server/`: `cargo run` compiles cleanly and prints something like `INFO calcifer_server: listening on 0.0.0.0:8080`.
2. `lsof -iTCP:8080 -sTCP:LISTEN` shows the process bound to the port.
3. `nc localhost 8080` connects (no protocol exchange, but the connection accepts) — confirming the listener is alive.
4. Ctrl-C exits cleanly with no panic message.
5. `cargo watch -x run` rebuilds when `src/main.rs` is edited.

## Done when

All 5 verification steps pass; `server/` compiles in under ~10s on a warm cache.

## What's next (Part 2)

Replace the raw `TcpListener` accept loop with `tonic::transport::Server`, wire `tonic-build` to compile the proto files, and stub out `EntityService::Get` returning a hardcoded entity.
