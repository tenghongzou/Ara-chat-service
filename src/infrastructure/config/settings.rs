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
    #[serde(default)]
    pub cors: CorsSettings,
    #[serde(default)]
    pub file_storage: FileStorageSettings,
    #[serde(default)]
    pub notification: NotificationSettings,
    #[serde(default)]
    pub gdpr: GdprSettings,
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

/// CORS settings for API access control
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CorsSettings {
    /// Allowed origins (empty = allow any in dev, deny all in prod)
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    /// Allow credentials (cookies, authorization headers)
    #[serde(default)]
    pub allow_credentials: bool,
    /// Max age for preflight cache (seconds)
    #[serde(default = "default_cors_max_age")]
    pub max_age_seconds: u64,
}

fn default_cors_max_age() -> u64 {
    3600 // 1 hour
}

/// Storage backend type
#[derive(Debug, Clone, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackend {
    #[default]
    Local,
    S3,
}

/// File storage settings
#[derive(Debug, Clone, Deserialize)]
pub struct FileStorageSettings {
    /// Storage backend: local or s3
    #[serde(default)]
    pub backend: StorageBackend,

    /// Maximum file size in bytes (default: 50MB)
    #[serde(default = "default_max_file_size")]
    pub max_file_size: usize,

    /// Allowed MIME types (empty = allow all common types)
    #[serde(default = "default_allowed_types")]
    pub allowed_types: Vec<String>,

    /// Enable thumbnail generation for images
    #[serde(default = "default_thumbnail_enabled")]
    pub thumbnail_enabled: bool,

    /// Maximum thumbnail dimension in pixels
    #[serde(default = "default_thumbnail_max_dimension")]
    pub thumbnail_max_dimension: u32,

    // Local storage settings
    /// Local storage path
    #[serde(default = "default_local_path")]
    pub local_path: String,

    // S3/MinIO settings
    /// S3 bucket name
    #[serde(default)]
    pub s3_bucket: Option<String>,
    /// S3 region
    #[serde(default)]
    pub s3_region: Option<String>,
    /// S3/MinIO endpoint URL (for MinIO)
    #[serde(default)]
    pub s3_endpoint: Option<String>,
    /// S3 access key
    #[serde(default)]
    pub s3_access_key: Option<String>,
    /// S3 secret key
    #[serde(default)]
    pub s3_secret_key: Option<String>,
    /// S3 public URL prefix (for generating public URLs)
    #[serde(default)]
    pub s3_public_url: Option<String>,
}

impl Default for FileStorageSettings {
    fn default() -> Self {
        Self {
            backend: StorageBackend::Local,
            max_file_size: default_max_file_size(),
            allowed_types: default_allowed_types(),
            thumbnail_enabled: default_thumbnail_enabled(),
            thumbnail_max_dimension: default_thumbnail_max_dimension(),
            local_path: default_local_path(),
            s3_bucket: None,
            s3_region: None,
            s3_endpoint: None,
            s3_access_key: None,
            s3_secret_key: None,
            s3_public_url: None,
        }
    }
}

fn default_max_file_size() -> usize {
    52_428_800 // 50MB
}

fn default_allowed_types() -> Vec<String> {
    vec![
        "image/jpeg".to_string(),
        "image/png".to_string(),
        "image/gif".to_string(),
        "image/webp".to_string(),
        "application/pdf".to_string(),
        "text/plain".to_string(),
        "application/zip".to_string(),
        "application/x-zip-compressed".to_string(),
    ]
}

fn default_thumbnail_enabled() -> bool {
    true
}

fn default_thumbnail_max_dimension() -> u32 {
    200
}

fn default_local_path() -> String {
    "./uploads".to_string()
}

/// Notification service integration settings
#[derive(Debug, Clone, Deserialize)]
pub struct NotificationSettings {
    /// Enable push notifications via Redis Pub/Sub
    #[serde(default = "default_notification_enabled")]
    pub enabled: bool,
    /// TTL for notification messages in seconds
    #[serde(default = "default_notification_ttl")]
    pub ttl_seconds: u32,
    /// Send notifications for new messages (to offline users)
    #[serde(default = "default_true")]
    pub notify_new_messages: bool,
    /// Send notifications for @mentions
    #[serde(default = "default_true")]
    pub notify_mentions: bool,
    /// Send notifications for emoji reactions
    #[serde(default = "default_true")]
    pub notify_reactions: bool,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            enabled: default_notification_enabled(),
            ttl_seconds: default_notification_ttl(),
            notify_new_messages: true,
            notify_mentions: true,
            notify_reactions: true,
        }
    }
}

fn default_notification_enabled() -> bool {
    true
}

fn default_notification_ttl() -> u32 {
    3600 // 1 hour
}

fn default_true() -> bool {
    true
}

/// GDPR compliance settings
#[derive(Debug, Clone, Deserialize)]
pub struct GdprSettings {
    /// Enable GDPR compliance features
    #[serde(default = "default_gdpr_enabled")]
    pub enabled: bool,
    /// Base path for export files
    #[serde(default = "default_gdpr_export_path")]
    pub export_path: String,
    /// Days to retain export files before cleanup
    #[serde(default = "default_gdpr_export_retention_days")]
    pub export_retention_days: u32,
    /// Years to retain audit logs (GDPR requires minimum 7 years)
    #[serde(default = "default_gdpr_audit_retention_years")]
    pub audit_log_retention_years: u32,
}

impl Default for GdprSettings {
    fn default() -> Self {
        Self {
            enabled: default_gdpr_enabled(),
            export_path: default_gdpr_export_path(),
            export_retention_days: default_gdpr_export_retention_days(),
            audit_log_retention_years: default_gdpr_audit_retention_years(),
        }
    }
}

fn default_gdpr_enabled() -> bool {
    true
}

fn default_gdpr_export_path() -> String {
    "./gdpr-exports".to_string()
}

fn default_gdpr_export_retention_days() -> u32 {
    7
}

fn default_gdpr_audit_retention_years() -> u32 {
    7
}

impl Settings {
    /// Create minimal settings for testing (no external dependencies)
    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_testing() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 0, // Will be assigned by OS
            jwt: JwtSettings {
                secret: "test-secret-key-that-is-at-least-32-characters-long".to_string(),
                issuer: Some("test-issuer".to_string()),
                audience: Some("test-audience".to_string()),
            },
            redis: RedisSettings::default(),
            database: DatabaseSettings {
                url: "postgres://test:test@localhost:5432/test".to_string(),
                max_connections: 5,
                min_connections: 1,
                run_migrations: false,
                migrations_path: "./migrations".to_string(),
                sharding_enabled: false,
                coordinator_url: None,
                worker_nodes: vec![],
                shard_count: 1024,
                acquire_timeout_seconds: 30,
                idle_timeout_seconds: 300,
            },
            websocket: WebSocketSettings::default(),
            cluster: ClusterSettings {
                enabled: false,
                server_id: "test-server".to_string(),
                session_prefix: "test:sessions".to_string(),
                routing_channel: "test:route".to_string(),
            },
            otel: OtelSettings::default(),
            cors: CorsSettings::default(),
            file_storage: FileStorageSettings::default(),
            notification: NotificationSettings::default(),
            gdpr: GdprSettings::default(),
        }
    }

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
