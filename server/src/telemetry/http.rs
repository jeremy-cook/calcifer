//! The layer around each listener, outside the local-only checks so a refused
//! request is seen too: an HTTP server span (`axum-tracing-opentelemetry`) for
//! what isn't gRPC, and `http.server.request.duration` for everything.

use std::sync::Arc;
use std::time::Instant;

use axum::extract::{MatchedPath, Request, State};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum_tracing_opentelemetry::middleware::OtelAxumLayer;
use opentelemetry::metrics::{Histogram, Meter};
use opentelemetry::KeyValue;
use opentelemetry_semantic_conventions::attribute::{
    ERROR_TYPE, HTTP_REQUEST_METHOD, HTTP_RESPONSE_STATUS_CODE, HTTP_ROUTE,
};
use opentelemetry_semantic_conventions::metric::HTTP_SERVER_REQUEST_DURATION;

/// Adds the layer to a listener's whole router, recording on `meter`.
pub fn layer(router: axum::Router, meter: &Meter) -> axum::Router {
    let duration = meter
        .f64_histogram(HTTP_SERVER_REQUEST_DURATION)
        .with_description("Duration of an HTTP request, to its response headers")
        .with_unit("s")
        .build();
    router
        .layer(middleware::from_fn_with_state(Arc::new(duration), record))
        .layer(OtelAxumLayer::default().filter(not_rpc))
}

async fn record(State(duration): State<Arc<Histogram<f64>>>, req: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = req.method().as_str().to_owned();
    // The route template, not the path, so ids don't become label values. The
    // UI's fallback has none.
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned());
    let res = next.run(req).await;

    let status = res.status();
    let mut attributes = vec![
        KeyValue::new(HTTP_REQUEST_METHOD, method),
        KeyValue::new(HTTP_RESPONSE_STATUS_CODE, i64::from(status.as_u16())),
    ];
    if let Some(route) = route {
        attributes.push(KeyValue::new(HTTP_ROUTE, route));
    }
    if status.is_server_error() {
        attributes.push(KeyValue::new(ERROR_TYPE, status.as_str().to_owned()));
    }
    duration.record(started.elapsed().as_secs_f64(), &attributes);
    res
}

/// gRPC-Web under `/api`, native gRPC and reflection at their services' paths:
/// those get their span from [`super::grpc`].
fn not_rpc(path: &str) -> bool {
    !["/api/", "/calcifer.v1.", "/grpc.reflection."]
        .iter()
        .any(|prefix| path.starts_with(prefix))
}
