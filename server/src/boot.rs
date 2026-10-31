//! Telemetry at start-up: the one place that reads its configuration, chooses
//! the exporters and owns the providers (ADR 13). It installs
//! [`crate::telemetry`]'s subscriber with the sinks it built, and gives the rest
//! of the server the meter to record on.
//!
//! - `OTEL_EXPORTER_OTLP_ENDPOINT`: export traces, logs and metrics over
//!   OTLP/gRPC to it. Unset, nothing is exported and the meter is a no-op.
//! - `OTEL_SERVICE_NAME`, `OTEL_RESOURCE_ATTRIBUTES` and the other standard
//!   `OTEL_*` variables, read by the SDK.
//! - `RUST_LOG`: the filter every sink applies, `info` by default.

use opentelemetry::metrics::{Meter, MeterProvider, NoopMeterProvider};
use opentelemetry::trace::TracerProvider;
use opentelemetry::{global, InstrumentationScope, KeyValue};
use opentelemetry_otlp::{LogExporter, MetricExporter, SpanExporter};
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::resource::SERVICE_VERSION;
use opentelemetry_semantic_conventions::SCHEMA_URL;

use crate::telemetry::{self, Sinks};

/// `service.name` unless `OTEL_SERVICE_NAME` says otherwise.
const SERVICE_NAME: &str = "calcifer-server";

/// The providers telemetry is exported through; all `None` when it isn't.
/// Keep it until the listeners stop, then [`Telemetry::shutdown`] it.
pub struct Telemetry {
    tracer: Option<SdkTracerProvider>,
    meter: Option<SdkMeterProvider>,
    logger: Option<SdkLoggerProvider>,
}

impl Telemetry {
    /// Reads the configuration, builds the exporters it asks for and installs
    /// the `tracing` subscriber. Call once, first, inside the Tokio runtime
    /// (the OTLP exporter's gRPC client needs it).
    pub fn start() -> anyhow::Result<Self> {
        // An incoming `traceparent` continues its trace. The span layers read
        // the propagator from the global; they take it no other way.
        global::set_text_map_propagator(TraceContextPropagator::new());

        let providers = match std::env::var_os("OTEL_EXPORTER_OTLP_ENDPOINT") {
            Some(_) => Self::otlp()?,
            None => Self {
                tracer: None,
                meter: None,
                logger: None,
            },
        };
        let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
        telemetry::install(Sinks {
            filter: &filter,
            stdout: true,
            tracer: providers
                .tracer
                .as_ref()
                .map(|p| p.tracer_with_scope(scope())),
            logger: providers.logger.as_ref(),
        })?;
        Ok(providers)
    }

    /// Exports all three signals over OTLP/gRPC, to the endpoint the SDK reads
    /// from `OTEL_EXPORTER_OTLP_ENDPOINT`.
    fn otlp() -> anyhow::Result<Self> {
        let resource = resource();
        Ok(Self {
            tracer: Some(
                SdkTracerProvider::builder()
                    .with_batch_exporter(SpanExporter::builder().with_tonic().build()?)
                    .with_resource(resource.clone())
                    .build(),
            ),
            meter: Some(
                SdkMeterProvider::builder()
                    .with_periodic_exporter(MetricExporter::builder().with_tonic().build()?)
                    .with_resource(resource.clone())
                    .build(),
            ),
            logger: Some(
                SdkLoggerProvider::builder()
                    .with_batch_exporter(LogExporter::builder().with_tonic().build()?)
                    .with_resource(resource)
                    .build(),
            ),
        })
    }

    /// The meter the server records on: a no-op one when nothing is exported.
    pub fn meter(&self) -> Meter {
        match &self.meter {
            Some(provider) => provider.meter_with_scope(scope()),
            None => NoopMeterProvider::new().meter_with_scope(scope()),
        }
    }

    /// Flushes what hasn't been exported yet. Call last, after the listeners
    /// have stopped.
    pub fn shutdown(self) {
        let results = [
            ("traces", self.tracer.map(|p| p.shutdown())),
            ("metrics", self.meter.map(|p| p.shutdown())),
            ("logs", self.logger.map(|p| p.shutdown())),
        ];
        for (signal, result) in results {
            if let Some(Err(error)) = result {
                eprintln!("telemetry: flushing {signal} failed: {error}");
            }
        }
    }
}

/// The instrumentation scope of the server's own spans and instruments.
fn scope() -> InstrumentationScope {
    InstrumentationScope::builder(env!("CARGO_PKG_NAME"))
        .with_version(env!("CARGO_PKG_VERSION"))
        .with_schema_url(SCHEMA_URL)
        .build()
}

/// The SDK's defaults (`OTEL_SERVICE_NAME`, `OTEL_RESOURCE_ATTRIBUTES`) plus
/// our name, when none was given, and version.
fn resource() -> Resource {
    let builder = Resource::builder();
    let builder = if std::env::var_os("OTEL_SERVICE_NAME").is_some() {
        builder
    } else {
        builder.with_service_name(SERVICE_NAME)
    };
    builder
        .with_attribute(KeyValue::new(SERVICE_VERSION, env!("CARGO_PKG_VERSION")))
        .build()
}
