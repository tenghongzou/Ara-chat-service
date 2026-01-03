//! API middleware - request ID propagation and other middleware
//!
//! Provides request ID generation and propagation for distributed tracing.

use axum::{
    extract::Request,
    http::{header::HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

/// Header name for request ID
pub static REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

/// Extension type for storing request ID in request extensions
#[derive(Clone, Debug)]
pub struct RequestId(pub String);

impl RequestId {
    /// Create a new random request ID
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Get the request ID string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Middleware that extracts or generates a request ID
///
/// If the request contains an `X-Request-ID` header, it will be used.
/// Otherwise, a new UUID will be generated.
///
/// The request ID is:
/// 1. Added to request extensions for use in handlers
/// 2. Added to response headers for client tracing
/// 3. Added to tracing span for log correlation
pub async fn request_id_middleware(mut request: Request, next: Next) -> Response {
    // Extract or generate request ID
    let request_id = request
        .headers()
        .get(&REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| RequestId(s.to_string()))
        .unwrap_or_else(RequestId::new);

    // Add to request extensions
    request.extensions_mut().insert(request_id.clone());

    // Create a tracing span with the request ID
    let span = tracing::info_span!(
        "request",
        request_id = %request_id,
        method = %request.method(),
        uri = %request.uri(),
    );

    // Process request within the span
    let _guard = span.enter();

    let mut response = next.run(request).await;

    // Add request ID to response headers
    if let Ok(value) = HeaderValue::from_str(&request_id.0) {
        response.headers_mut().insert(REQUEST_ID_HEADER.clone(), value);
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_id_new() {
        let id1 = RequestId::new();
        let id2 = RequestId::new();

        // Should be valid UUIDs
        assert!(Uuid::parse_str(&id1.0).is_ok());
        assert!(Uuid::parse_str(&id2.0).is_ok());

        // Should be unique
        assert_ne!(id1.0, id2.0);
    }

    #[test]
    fn test_request_id_default() {
        let id = RequestId::default();
        assert!(Uuid::parse_str(&id.0).is_ok());
    }

    #[test]
    fn test_request_id_as_str() {
        let id = RequestId("test-123".to_string());
        assert_eq!(id.as_str(), "test-123");
    }

    #[test]
    fn test_request_id_display() {
        let id = RequestId("test-456".to_string());
        assert_eq!(format!("{}", id), "test-456");
    }

    #[test]
    fn test_request_id_debug() {
        let id = RequestId("test-789".to_string());
        let debug = format!("{:?}", id);
        assert!(debug.contains("test-789"));
    }

    #[test]
    fn test_request_id_clone() {
        let id1 = RequestId("original".to_string());
        let id2 = id1.clone();
        assert_eq!(id1.0, id2.0);
    }

    #[test]
    fn test_request_id_header_name() {
        assert_eq!(REQUEST_ID_HEADER.as_str(), "x-request-id");
    }
}
