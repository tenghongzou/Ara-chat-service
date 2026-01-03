//! API layer - HTTP and WebSocket endpoints

mod health;
mod routes;
mod rest;
mod websocket;

pub use health::health_check;
pub use routes::create_router;
pub use websocket::websocket_handler;
