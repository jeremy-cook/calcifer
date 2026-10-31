//! Logs, traces and metrics (ADR 13). `tracing` is the one API the rest of the
//! server writes to; this module routes it to whatever sinks its caller hands
//! over.
//!
//! It knows nothing about configuration, exporters or where the telemetry
//! goes: the caller (`crate::boot`) builds the sinks and the meter and passes
//! them in. A missing sink is simply not installed, and a no-op meter records
//! nothing, so stdout-only, OTLP and a test's in-memory exporter all look the
//! same from here. Nothing in this module reads a global.

pub mod metrics;

use std::fmt;

use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::trace::Tracer;
use tracing::{Event, Subscriber};
use tracing_subscriber::filter::{filter_fn, EnvFilter, FilterExt};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::{FormatTime, SystemTime};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

/// Crates whose events must not become log records sent to an exporter: the
/// exporter's own stack would log about each export it makes.
const EXPORT_STACK: [&str; 5] = ["opentelemetry", "tonic", "h2", "hyper", "tower"];

/// Where `tracing` output goes. Each sink is optional.
pub struct Sinks<'a> {
    /// `EnvFilter` directives every sink applies, such as `info`.
    pub filter: &'a str,
    /// Events as lines on stdout.
    pub stdout: bool,
    /// Spans, as OTEL spans from this tracer.
    pub tracer: Option<Tracer>,
    /// Events, as OTEL log records carrying the active trace and span ids.
    pub logger: Option<&'a SdkLoggerProvider>,
}

/// Installs the global `tracing` subscriber with the given sinks. Call once,
/// before anything logs.
pub fn install(sinks: Sinks<'_>) -> anyhow::Result<()> {
    let filter = || EnvFilter::try_new(sinks.filter);
    let stdout = sinks
        .stdout
        .then(|| {
            anyhow::Ok(
                tracing_subscriber::fmt::layer()
                    .event_format(Line)
                    .with_filter(filter()?),
            )
        })
        .transpose()?;
    // Spans → traces.
    let traces = sinks
        .tracer
        .map(|tracer| {
            anyhow::Ok(
                tracing_opentelemetry::layer()
                    .with_tracer(tracer)
                    .with_filter(filter()?),
            )
        })
        .transpose()?;
    // Events → log records.
    let logs = sinks
        .logger
        .map(|logger| {
            anyhow::Ok(
                OpenTelemetryTracingBridge::new(logger)
                    .with_filter(filter()?.and(filter_fn(|m| !in_export_stack(m.target())))),
            )
        })
        .transpose()?;

    tracing_subscriber::registry()
        .with(stdout)
        .with(traces)
        .with(logs)
        .try_init()?;
    Ok(())
}

/// A stdout line: time, level, target and the event's own fields. The default
/// formats repeat every enclosing span's fields on each line, which for a
/// request's events is the whole request (user agent and all); the trace keeps
/// that context.
struct Line;

impl<S, N> FormatEvent<S, N> for Line
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'w> FormatFields<'w> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let metadata = event.metadata();
        SystemTime.format_time(&mut writer)?;
        write!(writer, " {:>5} {}: ", metadata.level(), metadata.target())?;
        ctx.format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

fn in_export_stack(target: &str) -> bool {
    EXPORT_STACK.iter().any(|krate| {
        target
            .strip_prefix(krate)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with("::") || rest.starts_with('_'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_export_stack_is_matched_by_crate_not_by_prefix() {
        assert!(in_export_stack("opentelemetry_sdk::trace"));
        assert!(in_export_stack("h2::proto"));
        assert!(in_export_stack("tonic"));
        assert!(!in_export_stack("calcifer_server::embed"));
        assert!(!in_export_stack("hyperbole"));
    }
}
