//! Redis connection pool

use redis::aio::MultiplexedConnection;
use redis::Client;

use crate::config::RedisSettings;

/// Redis connection pool
pub struct RedisPool {
    client: Client,
}

impl RedisPool {
    /// Create a new Redis pool
    pub fn new(config: &RedisSettings) -> Result<Self, redis::RedisError> {
        let client = Client::open(config.url.as_str())?;

        tracing::info!(
            url = %config.url,
            "Redis client created"
        );

        Ok(Self { client })
    }

    /// Get a multiplexed connection
    pub async fn get_connection(&self) -> Result<MultiplexedConnection, redis::RedisError> {
        self.client.get_multiplexed_async_connection().await
    }

    /// Get the underlying Redis client (for pub/sub)
    pub fn get_client(&self) -> Result<Client, redis::RedisError> {
        Ok(self.client.clone())
    }

    /// Check if Redis is healthy
    pub async fn is_healthy(&self) -> bool {
        match self.get_connection().await {
            Ok(mut conn) => {
                redis::cmd("PING")
                    .query_async::<String>(&mut conn)
                    .await
                    .is_ok()
            }
            Err(_) => false,
        }
    }
}
