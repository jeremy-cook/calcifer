//! Requests must come from this machine (ADR 5). Binding loopback keeps other
//! machines out, but not a web page open in a browser on this one:
//!
//! - DNS rebinding points an attacker's name at 127.0.0.1, so the page's
//!   requests reach us same-origin. Its `Host` header still carries the
//!   attacker's name, so both listeners refuse any host but this machine (403).
//! - A cross-site `no-cors` POST with a `text/plain` body needs no preflight,
//!   so it would run an MCP tool. MCP refuses a foreign `Origin` (403) and any
//!   body that isn't `application/json` (415), which a page can only send
//!   after a CORS preflight that nothing here answers.
//!
//! The UI's API needs no Origin check: gRPC-Web posts `application/grpc-web*`,
//! which also takes a preflight.

use std::net::{IpAddr, SocketAddr};

use axum::extract::{Request, State};
use axum::http::uri::Authority;
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;

/// The UI and gRPC-Web listener: only the host check.
pub fn app(router: Router, listener: SocketAddr) -> Router {
    router.layer(middleware::from_fn_with_state(listener, check_host))
}

/// The MCP listener: the host check, then no browser request from another site.
pub fn mcp(router: Router, listener: SocketAddr) -> Router {
    router
        .layer(middleware::from_fn(check_content_type))
        .layer(middleware::from_fn(check_origin))
        .layer(middleware::from_fn_with_state(listener, check_host))
}

async fn check_host(State(listener): State<SocketAddr>, req: Request, next: Next) -> Response {
    // HTTP/1.1 sends `Host`; HTTP/2 puts it in the URI's authority.
    let host = match req.headers().get(header::HOST) {
        Some(value) => value.to_str().ok(),
        None => req.uri().authority().map(Authority::as_str),
    };
    if !host.is_some_and(|h| host_allowed(h, listener)) {
        return (StatusCode::FORBIDDEN, "Host is not this machine").into_response();
    }
    next.run(req).await
}

async fn check_origin(req: Request, next: Next) -> Response {
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .map(|v| v.to_str().unwrap_or(""));
    if !origin.is_none_or(origin_allowed) {
        return (StatusCode::FORBIDDEN, "Origin is not this machine").into_response();
    }
    next.run(req).await
}

async fn check_content_type(req: Request, next: Next) -> Response {
    if req.method() == Method::POST && !is_json(req.headers()) {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Content-Type must be application/json",
        )
            .into_response();
    }
    next.run(req).await
}

/// `application/json`, with or without parameters such as `; charset=utf-8`.
fn is_json(headers: &HeaderMap) -> bool {
    let Some(value) = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let essence = value.split(';').next().unwrap_or("").trim();
    essence.eq_ignore_ascii_case("application/json")
}

/// The host of an authority (`host[:port]`), brackets and port removed, when it
/// is `localhost` or an IP; callers decide which IPs name this machine.
fn parse_host(authority: &str) -> Option<Host> {
    let host = authority
        .parse::<Authority>()
        .ok()?
        .host()
        .to_ascii_lowercase();
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host == "localhost" {
        return Some(Host::Localhost);
    }
    host.parse::<IpAddr>().ok().map(Host::Ip)
}

enum Host {
    Localhost,
    Ip(IpAddr),
}

/// A `Host` header that names this machine, at any port: `localhost`, a
/// loopback IP, or the listener's own IP when it's bound to one address that
/// isn't loopback (so `http://192.168.1.5:8080` works when bound there).
fn host_allowed(authority: &str, listener: SocketAddr) -> bool {
    let bound = listener.ip();
    match parse_host(authority) {
        Some(Host::Localhost) => true,
        Some(Host::Ip(ip)) => ip.is_loopback() || (ip == bound && !bound.is_unspecified()),
        None => false,
    }
}

/// An `http(s)` origin whose host is loopback, like the MCP Inspector's
/// `http://localhost:6274`. `null` (a sandboxed or file page) is not.
fn origin_allowed(origin: &str) -> bool {
    let Some(authority) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    match parse_host(authority) {
        Some(Host::Localhost) => true,
        Some(Host::Ip(ip)) => ip.is_loopback(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    #[test]
    fn hosts_must_name_this_machine() {
        let loopback = addr("127.0.0.1:8081");
        for ok in [
            "localhost",
            "localhost:8081",
            "LocalHost:1",
            "127.0.0.1",
            "127.0.0.1:8080",
            "[::1]",
            "[::1]:8081",
        ] {
            assert!(host_allowed(ok, loopback), "{ok}");
        }
        for bad in [
            "evil.example",
            "evil.example:8081",
            "localhost.evil.example",
            "192.168.1.5:8081",
            "::1",
            "",
            "[::1",
        ] {
            assert!(!host_allowed(bad, loopback), "{bad}");
        }
    }

    #[test]
    fn a_specific_listener_address_is_allowed_too() {
        assert!(host_allowed("192.168.1.5:8080", addr("192.168.1.5:8080")));
        assert!(host_allowed("[fe80::1]:8080", addr("[fe80::1]:8080")));
        assert!(!host_allowed("192.168.1.6:8080", addr("192.168.1.5:8080")));
        // Bound everywhere, the listener has no one address to allow.
        assert!(!host_allowed("0.0.0.0:8080", addr("0.0.0.0:8080")));
        assert!(!host_allowed("[::]:8080", addr("[::]:8080")));
        assert!(host_allowed("localhost:8080", addr("0.0.0.0:8080")));
    }

    #[test]
    fn origins_must_be_loopback_http() {
        for ok in [
            "http://localhost:6274",
            "https://localhost",
            "http://127.0.0.1:5173",
            "http://[::1]:6274",
            "http://LOCALHOST",
        ] {
            assert!(origin_allowed(ok), "{ok}");
        }
        for bad in [
            "null",
            "https://evil.example",
            "http://localhost.evil.example",
            "file://",
            "ws://localhost",
            "localhost",
            "",
        ] {
            assert!(!origin_allowed(bad), "{bad}");
        }
    }

    #[test]
    fn content_type_must_be_json() {
        let with = |v: &str| HeaderMap::from_iter([(header::CONTENT_TYPE, v.parse().unwrap())]);
        assert!(is_json(&with("application/json")));
        assert!(is_json(&with("Application/JSON; charset=utf-8")));
        assert!(!is_json(&with("text/plain")));
        assert!(!is_json(&with("application/json-seq")));
        assert!(!is_json(&HeaderMap::new()));
    }
}
