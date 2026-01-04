//! Configuration module

mod settings;

pub use settings::{
    Settings, JwtSettings, RedisSettings, DatabaseSettings,
    WebSocketSettings, ClusterSettings, OtelSettings, CorsSettings,
    FileStorageSettings, StorageBackend,
    EmailSettings, EmailBackend, EmailTemplateSettings,
    CompressionSettings, CompressionAlgorithm,
};
