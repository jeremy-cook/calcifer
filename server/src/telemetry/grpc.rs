//! The gRPC and gRPC-Web routes' layer: a server span per call
//! (`tonic-tracing-opentelemetry`, which also continues an incoming
//! `traceparent`), and around each call `rpc.server.call.duration` and an error
//! event, inside the call's span, for a server-side failure. The span is the
//! record of the call: nothing here logs the request or the response.
//!
//! A call is recorded when its response body ends or is dropped, not when its
//! headers go out: a native call's status is in the trailers, and a stream
//! (`WatchEntities`) lasts as long as its body. The call's span is held until
//! then too, so the span and the metric agree on the duration.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{ready, Context, Poll};
use std::time::Instant;

use axum::body::{Body, Bytes};
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::Response;
use http_body::{Body as _, Frame, SizeHint};
use opentelemetry::metrics::{Histogram, Meter};
use opentelemetry::KeyValue;
use opentelemetry_semantic_conventions::attribute::{
    ERROR_TYPE, RPC_METHOD, RPC_RESPONSE_STATUS_CODE, RPC_SYSTEM_NAME,
};
use opentelemetry_semantic_conventions::metric::RPC_SERVER_CALL_DURATION;
use tonic::Code;
use tonic_tracing_opentelemetry::middleware::server::OtelGrpcLayer;
use tracing::Span;

/// Adds the layer to a router of tonic services, recording on `meter`.
pub fn layer(router: axum::Router, meter: &Meter) -> axum::Router {
    let duration = meter
        .f64_histogram(RPC_SERVER_CALL_DURATION)
        .with_description("Duration of a gRPC call, to its last response frame")
        .with_unit("s")
        .build();
    router
        .layer(middleware::from_fn_with_state(Arc::new(duration), record))
        .layer(OtelGrpcLayer::default())
}

async fn record(State(duration): State<Arc<Histogram<f64>>>, req: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = rpc_method(req.uri().path());
    let res = next.run(req).await;
    let (parts, inner) = res.into_parts();
    let call = Call {
        started,
        method,
        span: Span::current(),
        duration,
        http_status: parts.status,
        // An error is answered trailers-only: its status is in the headers.
        status: tonic::Status::from_header_map(&parts.headers),
        finished: false,
    };
    Response::from_parts(parts, Body::new(Recorded { inner, call }))
}

/// One call, recorded when its response body is dropped.
struct Call {
    started: Instant,
    method: String,
    span: Span,
    duration: Arc<Histogram<f64>>,
    http_status: StatusCode,
    status: Option<tonic::Status>,
    /// The body reached its end, so the client saw the whole response.
    finished: bool,
}

impl Call {
    fn record(&self, finished: bool) {
        let code = resolve_code(self.status.as_ref(), self.http_status, finished);
        let elapsed = self.started.elapsed();
        let status = code_name(code);
        let error_type = server_error(code).then_some(status);

        let mut attributes = vec![
            KeyValue::new(RPC_SYSTEM_NAME, "grpc"),
            KeyValue::new(RPC_METHOD, self.method.clone()),
            KeyValue::new(RPC_RESPONSE_STATUS_CODE, status),
        ];
        if let Some(error_type) = error_type {
            attributes.push(KeyValue::new(ERROR_TYPE, error_type));
        }
        self.duration.record(elapsed.as_secs_f64(), &attributes);

        if let Some(error_type) = error_type {
            let message = self.status.as_ref().map_or("", tonic::Status::message);
            self.span.in_scope(|| {
                tracing::error!(error.type = error_type, grpc.message = message, "rpc failed");
            });
        }
    }
}

/// The response body, unchanged, watching for trailers and the end.
struct Recorded {
    inner: Body,
    call: Call,
}

impl http_body::Body for Recorded {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, axum::Error>>> {
        let this = self.get_mut();
        let frame = ready!(Pin::new(&mut this.inner).poll_frame(cx));
        match &frame {
            Some(Ok(frame)) => {
                if let Some(trailers) = frame.trailers_ref() {
                    this.call.status = this
                        .call
                        .status
                        .take()
                        .or_else(|| tonic::Status::from_header_map(trailers));
                    this.call.finished = true;
                }
            }
            Some(Err(_)) => {}
            None => this.call.finished = true,
        }
        Poll::Ready(frame)
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

impl Drop for Recorded {
    fn drop(&mut self) {
        // An empty body (a trailers-only error) is never polled.
        self.call
            .record(self.call.finished || self.inner.is_end_stream());
    }
}

/// `calcifer.v1.EntityService/GetEntity` from `/calcifer.v1.EntityService/GetEntity`;
/// `_OTHER`, as the convention asks, for a path that names no method.
fn rpc_method(path: &str) -> String {
    match path.trim_start_matches('/').split_once('/') {
        Some((service, method))
            if !service.is_empty() && !method.is_empty() && !method.contains('/') =>
        {
            format!("{service}/{method}")
        }
        _ => "_OTHER".to_string(),
    }
}

/// The call's status: the one the server sent; else, from a response that
/// isn't gRPC (an unknown path's 404), its HTTP status; else OK when the client
/// read to the end (gRPC-Web carries a success's trailers in the body), and
/// CANCELLED when it went away first.
fn resolve_code(status: Option<&tonic::Status>, http_status: StatusCode, finished: bool) -> Code {
    if let Some(status) = status {
        return status.code();
    }
    match http_status {
        StatusCode::OK if finished => Code::Ok,
        StatusCode::OK => Code::Cancelled,
        StatusCode::NOT_FOUND => Code::Unimplemented,
        _ => Code::Unknown,
    }
}

/// The codes the gRPC conventions count as a server-side error.
fn server_error(code: Code) -> bool {
    matches!(
        code,
        Code::Unknown
            | Code::DeadlineExceeded
            | Code::Unimplemented
            | Code::Internal
            | Code::Unavailable
            | Code::DataLoss
    )
}

/// The code's canonical name, as `rpc.response.status_code` wants it.
fn code_name(code: Code) -> &'static str {
    match code {
        Code::Ok => "OK",
        Code::Cancelled => "CANCELLED",
        Code::Unknown => "UNKNOWN",
        Code::InvalidArgument => "INVALID_ARGUMENT",
        Code::DeadlineExceeded => "DEADLINE_EXCEEDED",
        Code::NotFound => "NOT_FOUND",
        Code::AlreadyExists => "ALREADY_EXISTS",
        Code::PermissionDenied => "PERMISSION_DENIED",
        Code::ResourceExhausted => "RESOURCE_EXHAUSTED",
        Code::FailedPrecondition => "FAILED_PRECONDITION",
        Code::Aborted => "ABORTED",
        Code::OutOfRange => "OUT_OF_RANGE",
        Code::Unimplemented => "UNIMPLEMENTED",
        Code::Internal => "INTERNAL",
        Code::Unavailable => "UNAVAILABLE",
        Code::DataLoss => "DATA_LOSS",
        Code::Unauthenticated => "UNAUTHENTICATED",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_method_is_service_slash_method() {
        assert_eq!(
            rpc_method("/calcifer.v1.EntityService/GetEntity"),
            "calcifer.v1.EntityService/GetEntity"
        );
        assert_eq!(rpc_method("/"), "_OTHER");
        assert_eq!(rpc_method("/nope"), "_OTHER");
        assert_eq!(rpc_method("/a/b/c"), "_OTHER");
    }

    #[test]
    fn the_status_falls_back_to_how_the_response_ended() {
        let not_found = tonic::Status::not_found("x");
        assert_eq!(
            resolve_code(Some(&not_found), StatusCode::OK, true),
            Code::NotFound
        );
        assert_eq!(resolve_code(None, StatusCode::OK, true), Code::Ok);
        assert_eq!(resolve_code(None, StatusCode::OK, false), Code::Cancelled);
        assert_eq!(
            resolve_code(None, StatusCode::NOT_FOUND, true),
            Code::Unimplemented
        );
    }
}
