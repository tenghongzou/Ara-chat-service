//! Server module - application state and server creation

mod state;

pub use state::AppState;

use std::time::Duration;

use axum::Router;
use axum::http::{header, HeaderValue, Method};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::api::create_router;
use crate::config::CorsSettings;

/// Create the Axum application with all routes and middleware
pub fn create_app(state: AppState) -> Router {
    let cors = create_cors_layer(&state.settings.cors);

    create_router(state)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
}

/// Create CORS layer based on settings
fn create_cors_layer(settings: &CorsSettings) -> CorsLayer {
    let mut cors = CorsLayer::new();

    // Configure allowed origins
    if settings.allowed_origins.is_empty() {
        // Development mode: allow any origin (with warning)
        tracing::warn!(
            "CORS: No origins configured, allowing any origin. \
            Configure CHAT__CORS__ALLOWED_ORIGINS for production!"
        );
        cors = cors.allow_origin(Any);
    } else {
        // Production mode: restrict to configured origins
        let origins: Vec<HeaderValue> = settings
            .allowed_origins
            .iter()
            .filter_map(|origin: &String| {
                origin.parse::<HeaderValue>().ok().or_else(|| {
                    tracing::warn!(origin = %origin, "Invalid CORS origin, skipping");
                    None
                })
            })
            .collect();

        if origins.is_empty() {
            tracing::error!("No valid CORS origins configured, API will reject cross-origin requests");
        } else {
            tracing::info!(origins = ?settings.allowed_origins, "CORS origins configured");
        }

        cors = cors.allow_origin(origins);
    }

    // Configure allowed methods
    cors = cors.allow_methods([
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::DELETE,
        Method::OPTIONS,
    ]);

    // Configure allowed headers
    cors = cors.allow_headers([
        header::AUTHORIZATION,
        header::CONTENT_TYPE,
        header::ACCEPT,
        header::ORIGIN,
    ]);

    // Configure max age for preflight cache
    cors = cors.max_age(Duration::from_secs(settings.max_age_seconds));

    // Configure credentials
    if settings.allow_credentials {
        cors = cors.allow_credentials(true);
    }

    cors
}
