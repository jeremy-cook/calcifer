use std::net::SocketAddr;
use tonic::transport::Server;

mod db;
mod error;
mod links;
mod proto;
mod services;
mod watch;

use crate::proto::entity_service_server::EntityServiceServer;
use crate::proto::rich_text_service_server::RichTextServiceServer;
use crate::services::entity::EntityService;
use crate::services::richtext::RichTextService;
use crate::watch::WatchHub;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = db::connect(&database_url).await?;
    let hub = WatchHub::new();

    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
    tracing::info!("listening on {}", addr);

    // CorsLayer outermost (preflight bypasses tonic), GrpcWebLayer inner so the
    // browser can speak gRPC-Web over HTTP/1.1. accept_http1 is required for it.
    Server::builder()
        .accept_http1(true)
        .layer(tower_http::cors::CorsLayer::very_permissive())
        .layer(tonic_web::GrpcWebLayer::new())
        .add_service(EntityServiceServer::new(EntityService::new(
            pool.clone(),
            hub,
        )))
        .add_service(RichTextServiceServer::new(RichTextService::new(pool)))
        .serve(addr)
        .await?;

    Ok(())
}
