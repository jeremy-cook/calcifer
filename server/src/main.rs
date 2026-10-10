use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tonic::service::Routes;
use tower::Layer;
use tower_http::services::{ServeDir, ServeFile};

mod dates;
mod db;
mod embed;
mod error;
mod links;
mod mcp;
mod proto;
mod security;
mod services;
mod structures;
#[cfg(test)]
mod test_support;
mod tiptap;
mod watch;

use crate::proto::entity_service_server::EntityServiceServer;
use crate::proto::rich_text_service_server::RichTextServiceServer;
use crate::proto::search_service_server::SearchServiceServer;
use crate::proto::structure_service_server::StructureServiceServer;
use crate::services::entity::EntityService;
use crate::services::richtext::RichTextService;
use crate::services::search::SearchService;
use crate::services::structure::StructureService;
use crate::watch::WatchHub;

/// Where the UI and the gRPC-Web API listen unless `CALCIFER_ADDR` says otherwise.
/// Loopback by default: the service has no auth (ADR 5).
const DEFAULT_ADDR: &str = "127.0.0.1:8080";
/// Where MCP listens unless `CALCIFER_MCP_ADDR` says otherwise.
const DEFAULT_MCP_ADDR: &str = "127.0.0.1:8081";
/// The built UI (`pnpm build` in `calcifer/`), relative to the working
/// directory, unless `CALCIFER_UI_DIR` says otherwise.
const DEFAULT_UI_DIR: &str = "../calcifer/dist";

/// The built UI with client-side routing: any path that isn't a file is
/// answered with `index.html`, so a deep link like `/e/<id>` loads the app.
/// `None` when there is no build, which leaves the API and MCP running alone.
fn ui_service(dir: PathBuf) -> Option<ServeDir<ServeFile>> {
    let index = dir.join("index.html");
    if !index.is_file() {
        tracing::warn!(
            "no UI build at {} (run `pnpm build` in calcifer/, or set CALCIFER_UI_DIR); serving the API and MCP only",
            dir.display()
        );
        return None;
    }
    Some(ServeDir::new(dir).fallback(ServeFile::new(index)))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = db::connect(&database_url).await?;
    let hub = WatchHub::new();
    // Background embed worker (M8): loads the local model and drains the embed
    // queue. A disabled handle (model unavailable) keeps the server lexical-only.
    let embed = embed::spawn(pool.clone());

    let entity = Arc::new(EntityService::new(pool.clone(), hub.clone(), embed.clone()));
    let richtext = Arc::new(RichTextService::new(pool.clone(), hub, embed.clone()));
    let search = Arc::new(SearchService::new(pool, embed));

    // One process, two listeners over the same services:
    //   CALCIFER_ADDR      /api/*  gRPC-Web, for the browser, and /* the built UI
    //   CALCIFER_MCP_ADDR  /mcp    MCP over HTTP, for agents, calling the services in-process
    let api = api_router(entity.clone(), richtext.clone(), search.clone());

    let app_addr = bind_addr("CALCIFER_ADDR", DEFAULT_ADDR)?;
    let mcp_addr = bind_addr("CALCIFER_MCP_ADDR", DEFAULT_MCP_ADDR)?;

    // Both listeners only answer requests addressed to this machine (ADR 5).
    let mcp = security::local::mcp(mcp::router(mcp::Knowledge::new(entity, richtext, search)), mcp_addr);
    let ui_dir = std::env::var("CALCIFER_UI_DIR").unwrap_or_else(|_| DEFAULT_UI_DIR.to_string());
    let app = security::local::app(app_router(api, ui_service(PathBuf::from(ui_dir))), app_addr);

    let app_listener = tokio::net::TcpListener::bind(app_addr).await?;
    let mcp_listener = tokio::net::TcpListener::bind(mcp_addr).await?;
    tracing::info!("UI and API listening on {}", app_addr);
    tracing::info!("MCP listening on {}", mcp_addr);
    // Either server ending (it only does on error) takes the process down.
    tokio::try_join!(axum::serve(app_listener, app), axum::serve(mcp_listener, mcp))?;

    Ok(())
}

/// gRPC-Web for the browser (GrpcWebLayer lets it speak over HTTP/1.1). A path
/// under `/api` that names no RPC is a 404, rather than falling through to the UI.
fn api_router(entity: Arc<EntityService>, richtext: Arc<RichTextService>, search: Arc<SearchService>) -> axum::Router {
    let web = tonic_web::GrpcWebLayer::new();
    Routes::new(web.layer(EntityServiceServer::from_arc(entity)))
        .add_service(web.layer(RichTextServiceServer::from_arc(richtext)))
        .add_service(web.layer(SearchServiceServer::from_arc(search)))
        .add_service(web.layer(StructureServiceServer::new(StructureService)))
        .into_axum_router()
        .fallback(|| async { axum::http::StatusCode::NOT_FOUND })
}

/// The API under `/api`, and the UI, when there's a build, everywhere else.
fn app_router(api: axum::Router, ui: Option<ServeDir<ServeFile>>) -> axum::Router {
    let app = axum::Router::new().nest("/api", api);
    match ui {
        Some(ui) => app.fallback_service(ui),
        None => app,
    }
}

fn bind_addr(var: &str, default: &str) -> anyhow::Result<SocketAddr> {
    let addr = std::env::var(var).unwrap_or_else(|_| default.to_string());
    addr.parse().map_err(|e| anyhow::anyhow!("{var}={addr:?} isn't a host:port address: {e}"))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::*;
    use crate::embed::EmbedHandle;
    use crate::test_support::{entity_service, memory_pool, richtext_service};

    #[tokio::test]
    async fn an_unknown_api_path_is_a_404_not_the_ui() {
        let dir = std::env::temp_dir().join(format!("calcifer-ui-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.html"), "<!doctype html>").unwrap();

        let pool = memory_pool().await;
        let entity = Arc::new(entity_service(pool.clone()));
        let richtext = Arc::new(richtext_service(pool.clone(), WatchHub::new()));
        let search = Arc::new(SearchService::new(pool, EmbedHandle::disabled()));
        let app = app_router(api_router(entity, richtext, search), ui_service(dir.clone()));
        let status = |method: &str, uri: &str| {
            let req = Request::builder().method(method).uri(uri).body(Body::empty()).unwrap();
            let app = app.clone();
            async move { app.oneshot(req).await.unwrap().status() }
        };

        assert_eq!(status("GET", "/e/some-entity").await, StatusCode::OK, "a client-side route loads the UI");
        assert_eq!(status("GET", "/api/nope").await, StatusCode::NOT_FOUND);
        assert_eq!(status("POST", "/api/calcifer.v1.NoService/Nope").await, StatusCode::NOT_FOUND);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
