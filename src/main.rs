use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::time::timeout;

use ara_chat_service::config::Settings;
use ara_chat_service::server::{create_app, AppState};
use ara_chat_service::shutdown::GracefulShutdown;
use ara_chat_service::tasks::BackgroundTasks;
use ara_chat_service::telemetry::init_telemetry;

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration first (needed for telemetry config)
    let settings = Settings::new()?;

    // Initialize telemetry (tracing + optional OpenTelemetry)
    let _telemetry_guard = init_telemetry(&settings.otel)
        .expect("Failed to initialize telemetry");

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "Ara Chat Service starting"
    );

    // Create application state
    let state = AppState::new(settings.clone()).await;
    tracing::info!("Application state initialized");

    // Create shutdown signal
    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);

    // Start background tasks
    let background_tasks = BackgroundTasks::new(
        state.clone(),
        shutdown_tx.subscribe(),
    );
    let tasks_handle = tokio::spawn(async move {
        background_tasks.run().await;
    });

    // Create graceful shutdown handler
    let graceful_shutdown = GracefulShutdown::new(
        state.connection_manager.clone(),
        shutdown_tx.clone(),
    );

    // Create Axum app
    let app = create_app(state);

    // Start server
    let addr = settings.server_addr();
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(address = %addr, "Chat server listening");

    // Run server with graceful shutdown
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal_handler(shutdown_tx))
    .await?;

    // Execute graceful shutdown sequence
    let shutdown_result = graceful_shutdown.execute("Server shutting down").await;
    tracing::info!(
        clients_notified = shutdown_result.clients_notified,
        connections_closed = shutdown_result.connections_closed,
        duration_ms = shutdown_result.duration.as_millis(),
        "Graceful shutdown phase completed"
    );

    // Wait for background tasks to finish with timeout
    const SHUTDOWN_TIMEOUT_SECS: u64 = 30;
    tracing::info!(
        timeout_secs = SHUTDOWN_TIMEOUT_SECS,
        "Waiting for background tasks to finish..."
    );

    match timeout(Duration::from_secs(SHUTDOWN_TIMEOUT_SECS), tasks_handle).await {
        Ok(_) => {
            tracing::info!("All background tasks completed gracefully");
        }
        Err(_) => {
            tracing::warn!(
                timeout_secs = SHUTDOWN_TIMEOUT_SECS,
                "Shutdown timeout exceeded, forcing exit"
            );
        }
    }

    tracing::info!("Chat server shutdown complete");
    Ok(())
}

async fn shutdown_signal_handler(shutdown_tx: tokio::sync::broadcast::Sender<()>) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C, initiating graceful shutdown");
        }
        _ = terminate => {
            tracing::info!("Received terminate signal, initiating graceful shutdown");
        }
    }

    // Send shutdown signal
    let _ = shutdown_tx.send(());
}
