//! The agent's door into the knowledge base: an MCP server (streamable HTTP,
//! `POST /mcp`) hosted by the same process as the gRPC-Web API and the UI. Its
//! tools call the services in-process, so an agent's write takes the same path
//! as the browser's and shows up live over `WatchEntities`.
//!
//! Only the request/response half of the transport is implemented: every POST
//! is answered with one JSON body. There are no server-initiated messages, so
//! there is no stream for GET to open.
//!
//! This file is the transport and protocol; [`args`] holds the tools' argument
//! types, [`dispatch`] routes a call to [`tools`], and [`definitions`] describes
//! each tool to the client. [`markdown`] is the note/document converter.

mod args;
mod definitions;
mod dispatch;
mod markdown;
#[cfg(test)]
mod tests;
mod tools;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::mcp::tools::ToolError;

pub use tools::Knowledge;

use self::definitions::tool_definitions;
use self::dispatch::dispatch;

/// Newest first. A client's version is echoed back when we speak it.
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const PARSE_ERROR: i64 = -32700;

/// `POST /mcp`. Any other method on the path is answered 405 by the router.
pub fn router(knowledge: Knowledge) -> axum::Router {
    axum::Router::new()
        .route("/mcp", post(post_mcp))
        .with_state(knowledge)
}

async fn post_mcp(State(knowledge): State<Knowledge>, body: Bytes) -> Response {
    let Ok(message) = serde_json::from_slice::<Value>(&body) else {
        return json_response(&error_reply(&Value::Null, PARSE_ERROR, "invalid JSON"));
    };
    // A batch is answered with a batch of the replies; notifications have none.
    let reply = match message {
        Value::Array(messages) if messages.is_empty() => {
            Some(error_reply(&Value::Null, INVALID_REQUEST, "empty batch"))
        }
        Value::Array(messages) => {
            let replies: Vec<Value> = futures::future::join_all(
                messages.into_iter().map(|m| handle_message(&knowledge, m)),
            )
            .await
            .into_iter()
            .flatten()
            .collect();
            (!replies.is_empty()).then_some(Value::Array(replies))
        }
        message => handle_message(&knowledge, message).await,
    };
    match reply {
        Some(reply) => json_response(&reply),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

fn json_response(body: &Value) -> Response {
    (
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

fn error_reply(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// One JSON-RPC message in, its reply out. Notifications and stray responses
/// (no `method`, or no `id`) get none; anything that isn't an object is an error.
async fn handle_message(knowledge: &Knowledge, message: Value) -> Option<Value> {
    if !message.is_object() {
        return Some(error_reply(
            &Value::Null,
            INVALID_REQUEST,
            "not a JSON-RPC message",
        ));
    }
    let id = message.get("id").cloned();
    let Some(method) = message["method"].as_str() else {
        return id.map(|id| error_reply(&id, INVALID_REQUEST, "not a JSON-RPC request"));
    };
    let id = id?;
    let params = &message["params"];
    let outcome = match method {
        "initialize" => Ok(initialize(params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => call_tool(knowledge, params).await,
        _ => Err((METHOD_NOT_FOUND, format!("unknown method {method}"))),
    };
    Some(match outcome {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => error_reply(&id, code, &message),
    })
}

fn initialize(params: &Value) -> Value {
    let version = params["protocolVersion"]
        .as_str()
        .filter(|v| PROTOCOL_VERSIONS.contains(v))
        .unwrap_or(PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "calcifer", "version": env!("CARGO_PKG_VERSION") },
    })
}

async fn call_tool(knowledge: &Knowledge, params: &Value) -> Result<Value, (i64, String)> {
    let name = params["name"].as_str().unwrap_or_default();
    let args = params
        .get("arguments")
        .filter(|a| !a.is_null())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let outcome = match dispatch(knowledge, name, args).await {
        Ok(outcome) => outcome,
        Err(rejected) => return Err((INVALID_PARAMS, rejected)),
    };
    // A failed tool is a result the agent can read, not a protocol error.
    let (text, is_error) = match outcome {
        Ok(text) => (text, false),
        Err(ToolError(text)) => (text, true),
    };
    Ok(json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }))
}

/// Arguments that don't fit the tool (a misspelt or unknown one, a missing one)
/// are a failed tool the agent can read and fix, not a protocol error.
fn parse_args<T: DeserializeOwned>(tool: &str, args: Value) -> Result<T, ToolError> {
    serde_json::from_value(args)
        .map_err(|e| ToolError(format!("invalid arguments for {tool}: {e}")))
}
