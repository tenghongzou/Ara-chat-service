//! Redis fallback utilities for graceful degradation
//!
//! Provides utilities to handle Redis failures gracefully without blocking
//! core operations like messaging.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::redis::RedisPool;

/// Configuration for Redis fallback behavior
#[derive(Clone, Debug)]
pub struct FallbackConfig {
    /// Duration before attempting to reconnect after failure
    pub backoff_duration: Duration,
    /// Log level for degraded mode warnings
    pub warn_on_degraded: bool,
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            backoff_duration: Duration::from_secs(30),
            warn_on_degraded: true,
        }
    }
}

/// Redis fallback handler that tracks connection health and provides
/// graceful degradation when Redis is unavailable.
pub struct RedisFallback {
    redis: Option<Arc<RedisPool>>,
    degraded: AtomicBool,
    last_failure: AtomicU64,
    failure_count: AtomicU64,
    config: FallbackConfig,
}

impl RedisFallback {
    /// Create a new fallback handler
    pub fn new(redis: Option<Arc<RedisPool>>) -> Self {
        Self::with_config(redis, FallbackConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(redis: Option<Arc<RedisPool>>, config: FallbackConfig) -> Self {
        Self {
            redis,
            degraded: AtomicBool::new(false),
            last_failure: AtomicU64::new(0),
            failure_count: AtomicU64::new(0),
            config,
        }
    }

    /// Check if Redis is available (not in degraded mode)
    pub fn is_available(&self) -> bool {
        if self.redis.is_none() {
            return false;
        }

        if !self.degraded.load(Ordering::Relaxed) {
            return true;
        }

        // Check if backoff period has passed
        let last_failure = self.last_failure.load(Ordering::Relaxed);
        let now = Instant::now();
        let elapsed = Duration::from_millis(
            now.elapsed().as_millis() as u64 - last_failure
        );

        if elapsed >= self.config.backoff_duration {
            // Reset degraded mode to attempt reconnection
            self.degraded.store(false, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Get the Redis pool if available
    pub fn get_redis(&self) -> Option<&Arc<RedisPool>> {
        if self.is_available() {
            self.redis.as_ref()
        } else {
            None
        }
    }

    /// Record a Redis operation failure
    pub fn record_failure(&self, error: &str) {
        let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        self.degraded.store(true, Ordering::Relaxed);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.last_failure.store(now, Ordering::Relaxed);

        if self.config.warn_on_degraded {
            tracing::warn!(
                error = %error,
                failure_count = %count,
                backoff_seconds = %self.config.backoff_duration.as_secs(),
                "Redis operation failed, entering degraded mode"
            );
        }
    }

    /// Record a successful operation (resets degraded mode)
    pub fn record_success(&self) {
        if self.degraded.swap(false, Ordering::Relaxed) {
            tracing::info!("Redis recovered from degraded mode");
        }
    }

    /// Get the current failure count
    pub fn failure_count(&self) -> u64 {
        self.failure_count.load(Ordering::Relaxed)
    }

    /// Check if currently in degraded mode
    pub fn is_degraded(&self) -> bool {
        self.degraded.load(Ordering::Relaxed)
    }

    /// Execute a Redis operation with fallback
    ///
    /// Returns `Ok(Some(result))` on success, `Ok(None)` if Redis is unavailable
    /// or in degraded mode, and logs the failure.
    pub async fn with_fallback<T, F, Fut>(&self, operation: F) -> Option<T>
    where
        F: FnOnce(Arc<RedisPool>) -> Fut,
        Fut: std::future::Future<Output = Result<T, String>>,
    {
        let redis = match self.get_redis() {
            Some(r) => r.clone(),
            None => return None,
        };

        match operation(redis).await {
            Ok(result) => {
                self.record_success();
                Some(result)
            }
            Err(e) => {
                self.record_failure(&e);
                None
            }
        }
    }
}

impl Clone for RedisFallback {
    fn clone(&self) -> Self {
        Self {
            redis: self.redis.clone(),
            degraded: AtomicBool::new(self.degraded.load(Ordering::Relaxed)),
            last_failure: AtomicU64::new(self.last_failure.load(Ordering::Relaxed)),
            failure_count: AtomicU64::new(self.failure_count.load(Ordering::Relaxed)),
            config: self.config.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_config_default() {
        let config = FallbackConfig::default();
        assert_eq!(config.backoff_duration, Duration::from_secs(30));
        assert!(config.warn_on_degraded);
    }

    #[test]
    fn test_redis_fallback_new_without_redis() {
        let fallback = RedisFallback::new(None);
        assert!(!fallback.is_available());
        assert!(!fallback.is_degraded());
    }

    #[test]
    fn test_record_failure_increments_count() {
        let fallback = RedisFallback::new(None);
        assert_eq!(fallback.failure_count(), 0);

        fallback.record_failure("test error");
        assert_eq!(fallback.failure_count(), 1);

        fallback.record_failure("another error");
        assert_eq!(fallback.failure_count(), 2);
    }

    #[test]
    fn test_record_failure_sets_degraded() {
        let fallback = RedisFallback::new(None);
        assert!(!fallback.is_degraded());

        fallback.record_failure("test error");
        assert!(fallback.is_degraded());
    }

    #[test]
    fn test_record_success_clears_degraded() {
        let fallback = RedisFallback::new(None);
        fallback.record_failure("test error");
        assert!(fallback.is_degraded());

        fallback.record_success();
        assert!(!fallback.is_degraded());
    }

    #[test]
    fn test_clone_preserves_state() {
        let fallback = RedisFallback::new(None);
        fallback.record_failure("test");

        let cloned = fallback.clone();
        assert_eq!(cloned.failure_count(), fallback.failure_count());
        assert_eq!(cloned.is_degraded(), fallback.is_degraded());
    }

    #[test]
    fn test_get_redis_returns_none_when_unavailable() {
        let fallback = RedisFallback::new(None);
        assert!(fallback.get_redis().is_none());
    }

    #[test]
    fn test_fallback_with_custom_config() {
        let config = FallbackConfig {
            backoff_duration: Duration::from_secs(60),
            warn_on_degraded: false,
        };
        let fallback = RedisFallback::with_config(None, config);
        assert_eq!(fallback.config.backoff_duration, Duration::from_secs(60));
        assert!(!fallback.config.warn_on_degraded);
    }
}
