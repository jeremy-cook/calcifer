#![allow(
    clippy::result_large_err,
    reason = "tonic::Status is the error type of every handler and helper"
)]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt;
use tonic::service::Routes;
use tower::Layer;
use tower_http::services::{ServeDir, ServeFile};

mod boot;
mod dates;
mod db;
mod embed;
mod error;
mod links;
mod mcp;
mod security;
mod services;
mod structures;
mod telemetry;
#[cfg(test)]
mod test_support;
mod tiptap;
mod watch;

use crate::services::entity::EntityService;
use crate::services::richtext::RichTextService;
use crate::services::search::SearchService;
use crate::services::structure::StructureService;
use crate::watch::WatchHub;
use calcifer::v1::entity_service_server::EntityServiceServer;
use calcifer::v1::rich_text_service_server::RichTextServiceServer;
use calcifer::v1::search_service_server::SearchServiceServer;
use calcifer::v1::structure_service_server::StructureServiceServer;

/// Where the UI and the gRPC-Web API listen unless `CALCIFER_ADDR` says otherwise.
/// Loopback by default: the service has no auth (ADR 5).
const DEFAULT_ADDR: &str = "127.0.0.1:8080";
/// Where MCP listens unless `CALCIFER_MCP_ADDR` says otherwise.
const DEFAULT_MCP_ADDR: &str = "127.0.0.1:8081";
/// The built UI (`pnpm build` in `web/`), relative to the working
/// directory, unless `CALCIFER_UI_DIR` says otherwise.
const DEFAULT_UI_DIR: &str = "../web/dist";

/// How long shutdown waits for open requests, such as a browser tab's
/// `WatchEntities` stream, which never ends on its own.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// The built UI with client-side routing: any path that isn't a file is
/// answered with `index.html`, so a deep link like `/e/<id>` loads the app.
/// `None` when there is no build, which leaves the API and MCP running alone.
fn ui_service(dir: PathBuf) -> Option<ServeDir<ServeFile>> {
    let index = dir.join("index.html");
    if !index.is_file() {
        tracing::warn!(
            ui_dir = %dir.display(),
            "no UI build (run `pnpm build` in web/, or set CALCIFER_UI_DIR); serving the API and MCP only"
        );
        return None;
    }
    Some(ServeDir::new(dir).fallback(ServeFile::new(index)))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // .env may set OTEL_EXPORTER_OTLP_ENDPOINT, so read it first.
    dotenvy::dotenv().ok();
    let telemetry = boot::Telemetry::start()?;
    // Everything below records on this meter, handed down explicitly.
    let meter = telemetry.meter();

    let database_url = std::env::var("DATABASE_URL")?;
    let pool = db::connect(&database_url).await?;
    telemetry::metrics::observe_pool(&meter, &pool);
    let hub = WatchHub::new();
    let (subscribers, revision) = (hub.clone(), hub.clone());
    telemetry::metrics::observe_watch(
        &meter,
        move || subscribers.subscriber_count() as u64,
        move || revision.current_revision(),
    );
    // Background embed worker (M8): loads the local model and drains the embed
    // queue. A disabled handle (model unavailable) keeps the server lexical-only.
    let embed = embed::spawn(pool.clone());

    let entity = Arc::new(EntityService::new(pool.clone(), hub.clone(), embed.clone()));
    let richtext = Arc::new(RichTextService::new(pool.clone(), hub, embed.clone()));
    let search = Arc::new(SearchService::new(pool, embed));

    // One process, two listeners over the same services:
    //   CALCIFER_ADDR      /api/*  gRPC-Web, for the browser; /calcifer.v1.*/* native
    //                      gRPC with reflection, for grpcurl; and /* the built UI
    //   CALCIFER_MCP_ADDR  /mcp    MCP over HTTP, for agents, calling the services in-process
    let api = api_router(entity.clone(), richtext.clone(), search.clone());
    let grpc = grpc_router(entity.clone(), richtext.clone(), search.clone())?;

    let app_addr = bind_addr("CALCIFER_ADDR", DEFAULT_ADDR)?;
    let mcp_addr = bind_addr("CALCIFER_MCP_ADDR", DEFAULT_MCP_ADDR)?;

    // Both listeners only answer requests addressed to this machine (ADR 5).
    let mcp = security::local::mcp(
        mcp::router(mcp::Knowledge::new(entity, richtext, search)),
        mcp_addr,
    );
    let ui_dir = std::env::var("CALCIFER_UI_DIR").unwrap_or_else(|_| DEFAULT_UI_DIR.to_string());
    let app = security::local::app(
        app_router(api, grpc, ui_service(PathBuf::from(ui_dir))),
        app_addr,
    );

    let app_listener = tokio::net::TcpListener::bind(app_addr).await?;
    let mcp_listener = tokio::net::TcpListener::bind(mcp_addr).await?;
    tracing::info!(addr = %app_addr, "UI and API listening");
    tracing::info!(addr = %mcp_addr, "MCP listening");

    // Ctrl-C or SIGTERM stops both listeners, waits up to SHUTDOWN_GRACE for open
    // requests, then flushes telemetry. Either server ending on its own (it
    // only does on error) takes the process down.
    let shutdown = async {
        stop_signal().await;
        tracing::info!("shutting down");
    }
    .shared();
    let serving = async {
        tokio::try_join!(
            axum::serve(app_listener, app).with_graceful_shutdown(shutdown.clone()),
            axum::serve(mcp_listener, mcp).with_graceful_shutdown(shutdown.clone()),
        )
    };
    let grace = async {
        shutdown.clone().await;
        tokio::time::sleep(SHUTDOWN_GRACE).await;
    };
    let served = tokio::select! {
        served = serving => served.map(|_| ()),
        () = grace => Ok(()),
    };
    telemetry.shutdown();
    Ok(served?)
}

/// Ctrl-C, or SIGTERM (what `scripts/calcifer dev` stops the server with).
async fn stop_signal() {
    let mut terminate =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
    let terminated = async {
        match terminate.as_mut() {
            Some(signal) => {
                signal.recv().await;
            }
            None => std::future::pending().await,
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        () = terminated => {}
    }
}

/// gRPC-Web for the browser (GrpcWebLayer lets it speak over HTTP/1.1). A path
/// under `/api` that names no RPC is a 404, rather than falling through to the UI.
fn api_router(
    entity: Arc<EntityService>,
    richtext: Arc<RichTextService>,
    search: Arc<SearchService>,
) -> axum::Router {
    let web = tonic_web::GrpcWebLayer::new();
    Routes::new(web.layer(EntityServiceServer::from_arc(entity)))
        .add_service(web.layer(RichTextServiceServer::from_arc(richtext)))
        .add_service(web.layer(SearchServiceServer::from_arc(search)))
        .add_service(web.layer(StructureServiceServer::new(StructureService)))
        .into_axum_router()
        .fallback(|| async { axum::http::StatusCode::NOT_FOUND })
}

/// Native gRPC, for tools like grpcurl, with reflection so they need no proto
/// files. A gRPC client calls `/<package>.<Service>/<Method>` with no prefix, so
/// these routes sit at the root beside the UI rather than under `/api`. They take
/// HTTP/2 without TLS, which no browser speaks.
fn grpc_router(
    entity: Arc<EntityService>,
    richtext: Arc<RichTextService>,
    search: Arc<SearchService>,
) -> anyhow::Result<axum::Router> {
    let reflection = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(calcifer::FILE_DESCRIPTOR_SET)
        .build_v1()?;
    Ok(Routes::new(EntityServiceServer::from_arc(entity))
        .add_service(RichTextServiceServer::from_arc(richtext))
        .add_service(SearchServiceServer::from_arc(search))
        .add_service(StructureServiceServer::new(StructureService))
        .add_service(reflection)
        .into_axum_router())
}

/// gRPC-Web under `/api`, native gRPC at its services' paths, and the UI, when
/// there's a build, everywhere else.
fn app_router(
    api: axum::Router,
    grpc: axum::Router,
    ui: Option<ServeDir<ServeFile>>,
) -> axum::Router {
    let app = axum::Router::new().nest("/api", api).merge(grpc);
    match ui {
        Some(ui) => app.fallback_service(ui),
        None => app,
    }
}

fn bind_addr(var: &str, default: &str) -> anyhow::Result<SocketAddr> {
    let addr = std::env::var(var).unwrap_or_else(|_| default.to_string());
    addr.parse()
        .map_err(|e| anyhow::anyhow!("{var}={addr:?} isn't a host:port address: {e}"))
}

#[cfg(test)]
mod tests {
    use axum::body::{Body, Bytes};
    use axum::http::{HeaderMap, Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;
    use crate::embed::EmbedHandle;
    use crate::test_support::{entity_service, memory_pool, richtext_service};

    /// The app as `main` builds it, without the local-only layer, serving the UI
    /// from `ui_dir` when it has a build.
    async fn app(ui_dir: PathBuf) -> axum::Router {
        let pool = memory_pool().await;
        let entity = Arc::new(entity_service(pool.clone()));
        let richtext = Arc::new(richtext_service(pool.clone(), WatchHub::new()));
        let search = Arc::new(SearchService::new(pool, EmbedHandle::disabled()));
        app_router(
            api_router(entity.clone(), richtext.clone(), search.clone()),
            grpc_router(entity, richtext, search).unwrap(),
            ui_service(ui_dir),
        )
    }

    /// One native gRPC call of `path` with `message` as the only request
    /// message: the HTTP status, the response messages' bytes, and the trailers.
    async fn grpc_call(
        app: axum::Router,
        path: &str,
        message: &impl prost::Message,
    ) -> (StatusCode, Bytes, HeaderMap) {
        let encoded = message.encode_to_vec();
        let mut frame = vec![0];
        frame.extend_from_slice(&u32::try_from(encoded.len()).unwrap().to_be_bytes());
        frame.extend_from_slice(&encoded);
        let req = Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/grpc")
            .header("te", "trailers")
            .body(Body::from(frame))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let collected = res.into_body().collect().await.unwrap();
        let trailers = collected.trailers().cloned().unwrap_or_default();
        (status, collected.to_bytes(), trailers)
    }

    /// The first message of a gRPC response body.
    fn first_message<M: prost::Message + Default>(body: &Bytes) -> M {
        let len = u32::from_be_bytes(body[1..5].try_into().unwrap()) as usize;
        M::decode(&body[5..5 + len]).unwrap()
    }

    #[tokio::test]
    async fn an_unknown_api_path_is_a_404_not_the_ui() {
        let dir = std::env::temp_dir().join(format!("calcifer-ui-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.html"), "<!doctype html>").unwrap();

        let app = app(dir.clone()).await;
        let status = |method: &str, uri: &str| {
            let req = Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .unwrap();
            let app = app.clone();
            async move { app.oneshot(req).await.unwrap().status() }
        };

        assert_eq!(
            status("GET", "/e/some-entity").await,
            StatusCode::OK,
            "a client-side route loads the UI"
        );
        assert_eq!(status("GET", "/api/nope").await, StatusCode::NOT_FOUND);
        assert_eq!(
            status("POST", "/api/calcifer.v1.NoService/Nope").await,
            StatusCode::NOT_FOUND
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn native_grpc_is_served_at_the_root() {
        use calcifer::v1::{ListStructuresRequest, ListStructuresResponse};

        let (status, body, trailers) = grpc_call(
            app(PathBuf::from("no-ui-build")).await,
            "/calcifer.v1.StructureService/ListStructures",
            &ListStructuresRequest::default(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            trailers.get("grpc-status").map(|v| v.to_str().unwrap()),
            Some("0")
        );
        let res: ListStructuresResponse = first_message(&body);
        assert!(res.structures.iter().any(|s| s.r#type == "Note"));
    }

    #[tokio::test]
    async fn reflection_lists_the_services() {
        use tonic_reflection::pb::v1::server_reflection_request::MessageRequest;
        use tonic_reflection::pb::v1::server_reflection_response::MessageResponse;
        use tonic_reflection::pb::v1::{ServerReflectionRequest, ServerReflectionResponse};

        let (status, body, _) = grpc_call(
            app(PathBuf::from("no-ui-build")).await,
            "/grpc.reflection.v1.ServerReflection/ServerReflectionInfo",
            &ServerReflectionRequest {
                host: String::new(),
                message_request: Some(MessageRequest::ListServices(String::new())),
            },
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let res: ServerReflectionResponse = first_message(&body);
        let Some(MessageResponse::ListServicesResponse(list)) = res.message_response else {
            panic!("expected a service list, got {res:?}");
        };
        let mut names: Vec<String> = list.service.into_iter().map(|s| s.name).collect();
        names.sort();
        assert_eq!(
            names,
            [
                "calcifer.v1.EntityService",
                "calcifer.v1.RichTextService",
                "calcifer.v1.SearchService",
                "calcifer.v1.StructureService",
                "grpc.reflection.v1.ServerReflection",
            ]
        );
    }
}
