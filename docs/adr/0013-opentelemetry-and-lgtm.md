# 13. OpenTelemetry for logs, traces and metrics, viewed in Grafana LGTM

- **Status:** Accepted
- **Date:** 2026-10-31. Accepted 2026-10-31.

## Context

The server printed a handful of `tracing` lines to stdout and nothing else. An internal
error became `Status::internal` without a log line, nothing measured how long an RPC or an
MCP tool took, and the embed queue was invisible. With an agent writing over MCP beside
the browser, "what just happened, and how long did it take" needs an answer that doesn't
start with adding print statements.

## Decision

**The server emits OpenTelemetry logs, traces and metrics, exported over OTLP/gRPC when
`OTEL_EXPORTER_OTLP_ENDPOINT` is set, to a `grafana/otel-lgtm` deployment (Loki, Tempo,
Prometheus, Grafana) in Docker Desktop's Kubernetes, reached on localhost
(`deploy/k8s/infra`). The cluster holds the base infrastructure; during development the
server runs natively and exports to it.**

1. **`tracing` stays the one API in our code.** `tracing-opentelemetry` turns its spans
   into OTEL spans, `opentelemetry-appender-tracing` turns its events into OTEL log
   records carrying the active trace and span ids, and stdout keeps printing the events.
   No second logging API.
2. **Opt-in export.** Without the endpoint the server only writes stdout, as before. With
   it, the official `opentelemetry` SDK exports all three signals over OTLP/gRPC.
3. **The caller chooses the sinks; the telemetry code takes what it's given.** Boot code
   (`server/src/boot.rs`) is the only place that reads the configuration, builds exporters
   and owns the providers. It hands `telemetry::install` its sinks (stdout, a tracer, a
   logger, each optional) and passes the meter down to every layer, the embed worker
   and the gauges as an argument. `server/src/telemetry/` reads no environment variables,
   knows no exporter and reads no global, so stdout-only, OTLP and a test's in-memory
   exporter look the same to it. Where a value has to be global, the caller sets it:
   boot sets the text-map propagator, which the third-party span layers read. No tracer
   or meter provider is global.
4. **One gRPC stack.** `opentelemetry-otlp`'s gRPC exporter is built on tonic 0.14, so the
   server moved from tonic 0.12 / prost 0.13 / axum 0.7 to tonic 0.14 / prost 0.14 / axum
   0.8 rather than carry two of each.
5. **Middleware, not handlers.** Per-request telemetry lives in layers composed in
   `main.rs` (`server/src/telemetry/`): a gRPC layer (spans from
   `tonic-tracing-opentelemetry`, plus our call metric and an error event),
   an MCP layer that reads the JSON-RPC request and reply around the handler, and an HTTP
   layer around each listener (spans from `axum-tracing-opentelemetry`, plus our
   request-duration metric). `opentelemetry-instrumentation-tower` was dropped for the
   last: it takes its meter only from the global provider. Handlers carry no telemetry statements; at
   most an internal helper has `#[tracing::instrument(skip_all)]` so it shows as a child
   span. The embed worker, a background job rather than a request, is instrumented in its
   own loop, and each job's span links to the request that queued it.
6. **The span is the record of the call; `tracing` doesn't log the request.** A request's
   trace starts when the call arrives. Logging the request or the response with
   `tracing` at that point only repeats the span's start: the event and the span carry
   the same facts. `tracing` events belong inside the request, in middleware or a
   handler, where something happens (the gRPC layer's error event for an `INTERNAL`
   status, the embed worker's failure warning). A request/response log, if we want one,
   comes from its own logging middleware, not from `tracing`.
7. **Current semantic conventions.** Metrics follow semconv 1.42:
   `rpc.server.call.duration` with `rpc.system.name`, `rpc.method` (`Service/Method`) and
   `rpc.response.status_code`; `mcp.server.operation.duration` with `mcp.method.name`,
   `gen_ai.tool.name` and `error.type`; `http.server.request.duration`;
   `db.client.connection.count`. Our own instruments are `calcifer.embed.*` and
   `calcifer.watch.*`.
8. **No note content in telemetry.** Records carry ids, method and tool names, statuses
   and durations; never documents, markdown, search queries or tool arguments.
9. **The incoming `traceparent` is honoured**, so the web UI can join a trace later
   without server changes. The UI itself is not instrumented yet.

## Alternatives considered

- **`metrics` + a Prometheus exporter, `log` + a file.** Popular, but neither is OTEL,
  and neither carries the trace id that ties a log line to its trace.
- **OTLP over HTTP to stay on tonic 0.12.** It still pulls prost 0.14 through
  `opentelemetry-proto`, so it avoided only half the duplication. The upgrade was small:
  codegen moved to `tonic-prost-build`, and nothing else changed.
- **Instrumenting in the handlers** (a counter in `call_tool`, a log in
  `From<AppError> for Status`). Simpler at first; it mixes telemetry into every handler
  and is easy to forget on the next one.
- **A `tracing` event per request as the access log.** Tried first: each layer wrote an
  `info!` when the call ended. It duplicated the span (same method, status and duration),
  and as an event inside the span it also landed on the trace.
- **Scraping stdout into Loki.** Loses the trace correlation and adds a shipper.

## Consequences

- The binary is larger and starts the three OTLP pipelines when export is on. When it's
  off, boot passes a no-op meter and no tracer or logger, and only the stdout layer and
  the request spans remain.
- The RPC and MCP conventions are still marked experimental (the semconv crate gates them
  behind `semconv_experimental`; MCP's names moved to the GenAI conventions repository),
  so names may change in a later semconv release.
- The MCP layer buffers each request and reply body (bounded by axum's 2 MiB default for
  requests) to read them. Fine for a local, single-user server.
- A streamed RPC (`WatchEntities`) is recorded when its body ends, so its duration is how
  long the stream was open, and a client going away is `CANCELLED`.
- Shutdown now waits up to 5 seconds for open requests on Ctrl-C or SIGTERM, then flushes
  telemetry.
