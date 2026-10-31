//! The `/mcp` layer: reads the JSON-RPC request and reply around the handler,
//! which stays unaware of it. The handler runs in a span named for the
//! operation (`tools/call search_notes`), which gets the outcome as `error.type`
//! when it fails, and each message records `mcp.server.operation.duration`,
//! following the MCP conventions (`mcp.method.name`, `gen_ai.tool.name`,
//! `error.type`). The span is the record of the call: nothing here logs the
//! request or the reply.
//!
//! Tool arguments and results are note content: they are read here, never
//! recorded.

use std::sync::Arc;
use std::time::Instant;

use axum::body::{to_bytes, Body, Bytes};
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use opentelemetry::metrics::{Histogram, Meter};
use opentelemetry::KeyValue;
use opentelemetry_semantic_conventions::attribute::{ERROR_TYPE, RPC_RESPONSE_STATUS_CODE};
use serde_json::Value;
use tracing::field::Empty;
use tracing::Instrument;

// The MCP and GenAI conventions moved to their own repository; the
// semconv crate keeps their names only as deprecated constants.
const MCP_SERVER_OPERATION_DURATION: &str = "mcp.server.operation.duration";
const MCP_METHOD_NAME: &str = "mcp.method.name";
const GEN_AI_TOOL_NAME: &str = "gen_ai.tool.name";
const GEN_AI_OPERATION_NAME: &str = "gen_ai.operation.name";

/// `error.type` for a tool that ran and failed: a result the agent reads
/// (`isError`), not a protocol error.
const TOOL_ERROR: &str = "tool_error";

/// axum's default body limit, which the handler's `Bytes` extractor applies.
const BODY_LIMIT: usize = 2 * 1024 * 1024;

/// Adds the layer to the MCP router, recording on `meter`.
pub fn layer(router: axum::Router, meter: &Meter) -> axum::Router {
    let duration = meter
        .f64_histogram(MCP_SERVER_OPERATION_DURATION)
        .with_description("Duration of an MCP request, by method and tool")
        .with_unit("s")
        .build();
    router.layer(middleware::from_fn_with_state(Arc::new(duration), record))
}

async fn record(State(duration): State<Arc<Histogram<f64>>>, req: Request, next: Next) -> Response {
    let started = Instant::now();
    let (parts, body) = req.into_parts();
    let Ok(body) = to_bytes(body, BODY_LIMIT).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let operations = operations(&body);
    let req = Request::from_parts(parts, Body::from(body));
    // Not JSON-RPC: the outer layer's HTTP span covers it.
    if operations.is_empty() {
        return next.run(req).await;
    }

    let span = if let [op] = operations.as_slice() {
        tracing::info_span!(
            "mcp",
            otel.name = %op.span_name(),
            otel.kind = "server",
            mcp.method.name = %op.method,
            gen_ai.tool.name = op.tool.as_deref(),
            jsonrpc.request.id = op.id.as_ref().map(ToString::to_string),
            error.type = Empty,
        )
    } else {
        tracing::info_span!("mcp", otel.name = "batch", otel.kind = "server")
    };
    let res = next.run(req).instrument(span.clone()).await;

    let (parts, body) = res.into_parts();
    let body = to_bytes(body, usize::MAX).await.unwrap_or_default();
    let replies = replies(&body);
    let elapsed = started.elapsed();
    for op in &operations {
        let error = op.id.as_ref().and_then(|id| replies.error_for(id));
        if let (Some(error), [_]) = (&error, operations.as_slice()) {
            span.record("error.type", error.as_str());
        }
        op.record(&duration, error.as_deref(), elapsed);
    }
    Response::from_parts(parts, Body::from(body))
}

/// One JSON-RPC message the client sent.
struct Operation {
    method: String,
    /// The tool, for `tools/call`.
    tool: Option<String>,
    /// None for a notification, which gets no reply.
    id: Option<Value>,
}

impl Operation {
    /// `{mcp.method.name} {target}`, as the conventions name the span.
    fn span_name(&self) -> String {
        match &self.tool {
            Some(tool) => format!("{} {tool}", self.method),
            None => self.method.clone(),
        }
    }

    fn record(&self, duration: &Histogram<f64>, error: Option<&str>, elapsed: std::time::Duration) {
        let mut attributes = vec![KeyValue::new(MCP_METHOD_NAME, self.method.clone())];
        if let Some(tool) = &self.tool {
            attributes.push(KeyValue::new(GEN_AI_TOOL_NAME, tool.clone()));
            attributes.push(KeyValue::new(GEN_AI_OPERATION_NAME, "execute_tool"));
        }
        if let Some(error) = error {
            attributes.push(KeyValue::new(ERROR_TYPE, error.to_owned()));
            if error != TOOL_ERROR {
                attributes.push(KeyValue::new(RPC_RESPONSE_STATUS_CODE, error.to_owned()));
            }
        }
        duration.record(elapsed.as_secs_f64(), &attributes);
    }
}

/// The JSON-RPC requests and notifications in a body: one message or a batch.
fn operations(body: &Bytes) -> Vec<Operation> {
    let Ok(message) = serde_json::from_slice::<Value>(body) else {
        return Vec::new();
    };
    let messages = match message {
        Value::Array(messages) => messages,
        message => vec![message],
    };
    messages
        .iter()
        .filter_map(|m| {
            let method = m["method"].as_str()?.to_owned();
            let tool = (method == "tools/call")
                .then(|| m["params"]["name"].as_str().map(str::to_owned))
                .flatten();
            Some(Operation {
                method,
                tool,
                id: m.get("id").cloned(),
            })
        })
        .collect()
}

/// The replies in a response body, by request id.
struct Replies(Vec<Value>);

fn replies(body: &Bytes) -> Replies {
    Replies(match serde_json::from_slice::<Value>(body) {
        Ok(Value::Array(replies)) => replies,
        Ok(reply) => vec![reply],
        Err(_) => Vec::new(),
    })
}

impl Replies {
    /// The `error.type` of the reply to `id`: its JSON-RPC error code, or
    /// [`TOOL_ERROR`]; None when it succeeded.
    fn error_for(&self, id: &Value) -> Option<String> {
        let reply = self.0.iter().find(|r| r.get("id") == Some(id))?;
        if let Some(code) = reply["error"]["code"].as_i64() {
            return Some(code.to_string());
        }
        (reply["result"]["isError"] == Value::Bool(true)).then(|| TOOL_ERROR.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use opentelemetry::metrics::MeterProvider;
    use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData};
    use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};
    use tower::ServiceExt;

    use super::*;
    use crate::embed::EmbedHandle;
    use crate::mcp::{self, Knowledge};
    use crate::services::search::SearchService;
    use crate::test_support::{entity_service, memory_pool, richtext_service};
    use crate::watch::WatchHub;

    /// Posts each body to `/mcp` through the layer, then returns the
    /// `error.type` (None for success) and tool of every recorded operation.
    async fn recorded(bodies: &[&str]) -> Vec<(Option<String>, Option<String>)> {
        let exporter = InMemoryMetricExporter::default();
        let provider = SdkMeterProvider::builder()
            .with_reader(PeriodicReader::builder(exporter.clone()).build())
            .build();
        let pool = memory_pool().await;
        let knowledge = Knowledge::new(
            Arc::new(entity_service(pool.clone())),
            Arc::new(richtext_service(pool.clone(), WatchHub::new())),
            Arc::new(SearchService::new(pool, EmbedHandle::disabled())),
        );
        let app = layer(mcp::router(knowledge), &provider.meter("test"));
        for body in bodies {
            let req = axum::http::Request::post("/mcp")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap();
            app.clone().oneshot(req).await.unwrap();
        }
        provider.force_flush().unwrap();

        let mut seen = Vec::new();
        for resource in exporter.get_finished_metrics().unwrap() {
            for scope in resource.scope_metrics() {
                for metric in scope.metrics() {
                    let AggregatedMetrics::F64(MetricData::Histogram(h)) = metric.data() else {
                        continue;
                    };
                    for point in h.data_points() {
                        let attr = |key: &str| {
                            point
                                .attributes()
                                .find(|kv| kv.key.as_str() == key)
                                .map(|kv| kv.value.to_string())
                        };
                        seen.push((attr(ERROR_TYPE), attr(GEN_AI_TOOL_NAME)));
                    }
                }
            }
        }
        seen.sort();
        seen
    }

    #[tokio::test]
    async fn each_call_is_recorded_with_its_outcome() {
        let seen = recorded(&[
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_structures"}}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_note","arguments":{"name":"Nowhere"}}}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"no_such_tool"}}"#,
        ])
        .await;
        assert_eq!(
            seen,
            [
                (None, Some("list_structures".into())),
                (Some("-32602".into()), Some("no_such_tool".into())),
                (Some(TOOL_ERROR.into()), Some("get_note".into())),
            ]
        );
    }

    #[tokio::test]
    async fn a_batch_records_each_message() {
        let seen = recorded(&[
            r#"[{"jsonrpc":"2.0","id":1,"method":"ping"},{"jsonrpc":"2.0","id":2,"method":"nope"}]"#,
        ])
        .await;
        assert_eq!(seen, [(None, None), (Some("-32601".into()), None)]);
    }
}
