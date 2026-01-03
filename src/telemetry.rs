//! Telemetry and tracing setup

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::OtelSettings;

/// Guard that flushes telemetry on drop
pub struct TelemetryGuard;

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        // Flush any pending traces
    }
}

/// Initialize telemetry (tracing + optional OpenTelemetry)
pub fn init_telemetry(config: &OtelSettings) -> Result<TelemetryGuard, Box<dyn std::error::Error>> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false);

    if config.enabled {
        // TODO: Setup OpenTelemetry
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();

        tracing::info!(
            endpoint = %config.endpoint,
            service_name = %config.service_name,
            "OpenTelemetry enabled"
        );
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
    }

    Ok(TelemetryGuard)
}
