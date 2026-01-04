//! API layer - HTTP and WebSocket endpoints

mod attachment;
mod error;
mod health;
mod middleware;
mod routes;
mod rest;
mod websocket;

pub use error::{ApiError, ErrorResponse};
pub use health::health_check;
pub use middleware::{request_id_middleware, RequestId, REQUEST_ID_HEADER};
pub use routes::create_router;
pub use websocket::websocket_handler;
