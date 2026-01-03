//! Application settings

use std::net::SocketAddr;

use config::{Config, Environment, File};
use serde::Deserialize;

use crate::auth::JwtConfig;

/// Application settings
#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,

    pub jwt: JwtSettings,
    #[serde(default)]
    pub redis: RedisSettings,
    pub database: DatabaseSettings,
    #[serde(default)]
    pub websocket: WebSocketSettings,
    #[serde(default)]
    pub cluster: ClusterSettings,
    #[serde(default)]
    pub otel: OtelSettings,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8082
}

#[derive(Debug, Clone, Deserialize)]
pub struct JwtSettings {
    pub secret: String,
    #[serde(default)]
    pub issuer: Option<String>,
    #[serde(default)]
    pub audience: Option<String>,
}

impl From<&JwtSettings> for JwtConfig {
    fn from(settings: &JwtSettings) -> Self {
        Self {
            secret: settings.secret.clone(),
            issuer: settings.issuer.clone(),
            audience: settings.audience.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RedisSettings {
    #[serde(default = "default_redis_url")]
    pub url: String,
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,
    /// Enable Redis Cluster mode
    #[serde(default)]
    pub cluster_enabled: bool,
    /// Redis Cluster nodes (comma-separated or array)
    #[serde(default)]
    pub cluster_nodes: Vec<String>,
}

fn default_redis_url() -> String {
    "redis://localhost:6379".to_string()
}

fn default_pool_size() -> u32 {
    10
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseSettings {
    pub url: String,
    #[serde(default = "default_db_pool_size")]
    pub max_connections: u32,
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    #[serde(default)]
    pub run_migrations: bool,
    #[serde(default = "default_migrations_path")]
    pub migrations_path: String,
    /// Enable sharding mode (Citus)
    #[serde(default)]
    pub sharding_enabled: bool,
    /// Coordinator URL for Citus (defaults to main URL)
    #[serde(default)]
    pub coordinator_url: Option<String>,
    /// Worker node URLs (node_id=url format)
    #[serde(default)]
    pub worker_nodes: Vec<String>,
    /// Number of shards (default: 1024)
    #[serde(default = "default_shard_count")]
    pub shard_count: u32,
    /// Connection acquire timeout in seconds
    #[serde(default = "default_acquire_timeout")]
    pub acquire_timeout_seconds: u64,
    /// Connection idle timeout in seconds
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_seconds: u64,
}

fn default_min_connections() -> u32 {
    5
}

fn default_shard_count() -> u32 {
    1024
}

fn default_acquire_timeout() -> u64 {
    30
}

fn default_idle_timeout() -> u64 {
    300
}

fn default_db_pool_size() -> u32 {
    20
}

fn default_migrations_path() -> String {
    "./migrations".to_string()
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct WebSocketSettings {
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    #[serde(default = "default_max_connections_per_user")]
    pub max_connections_per_user: usize,
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_seconds: u64,
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout_seconds: u64,
}

fn default_max_connections() -> usize {
    100_000
}

fn default_max_connections_per_user() -> usize {
    5
}

fn default_heartbeat_interval() -> u64 {
    30
}

fn default_connection_timeout() -> u64 {
    60
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ClusterSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_server_id")]
    pub server_id: String,
    #[serde(default = "default_session_prefix")]
    pub session_prefix: String,
    #[serde(default = "default_routing_channel")]
    pub routing_channel: String,
}

fn default_server_id() -> String {
    format!("chat-{}", uuid::Uuid::new_v4().to_string()[..8].to_string())
}

fn default_session_prefix() -> String {
    "chat:cluster:sessions".to_string()
}

fn default_routing_channel() -> String {
    "chat:cluster:route".to_string()
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct OtelSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_otel_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_service_name")]
    pub service_name: String,
}

fn default_otel_endpoint() -> String {
    "http://localhost:4317".to_string()
}

fn default_service_name() -> String {
    "ara-chat-service".to_string()
}

impl Settings {
    /// Load settings from environment and config files
    pub fn new() -> Result<Self, config::ConfigError> {
        let run_mode = std::env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        let s = Config::builder()
            // Start with defaults
            .set_default("host", "0.0.0.0")?
            .set_default("port", 8082)?
            // Add optional config files
            .add_source(File::with_name("config/default").required(false))
            .add_source(File::with_name(&format!("config/{}", run_mode)).required(false))
            // Add environment variables (with CHAT_ prefix)
            .add_source(
                Environment::with_prefix("CHAT")
                    .separator("__")
                    .try_parsing(true),
            )
            // Also support common env vars
            .add_source(
                Environment::default()
                    .prefix("")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;

        s.try_deserialize()
    }

    /// Get the server socket address
    pub fn server_addr(&self) -> SocketAddr {
        format!("{}:{}", self.host, self.port)
            .parse()
            .expect("Invalid server address")
    }
}
