//! Circuit breaker pattern for graceful degradation
//!
//! Provides fault tolerance for external service calls (Redis, PostgreSQL, etc.)
//! with automatic recovery and metrics integration.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use crate::metrics::{self, CircuitState};

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Name for metrics and logging
    pub name: String,
    /// Number of failures before opening circuit
    pub failure_threshold: u32,
    /// Number of successes in half-open state to close
    pub success_threshold: u32,
    /// Time to wait before transitioning from open to half-open
    pub reset_timeout: Duration,
    /// Timeout for individual operations
    pub call_timeout: Duration,
    /// Sliding window size for failure rate calculation
    pub window_size: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            failure_threshold: 5,
            success_threshold: 3,
            reset_timeout: Duration::from_secs(30),
            call_timeout: Duration::from_secs(5),
            window_size: Duration::from_secs(60),
        }
    }
}

impl CircuitBreakerConfig {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..Default::default()
        }
    }

    pub fn with_failure_threshold(mut self, threshold: u32) -> Self {
        self.failure_threshold = threshold;
        self
    }

    pub fn with_success_threshold(mut self, threshold: u32) -> Self {
        self.success_threshold = threshold;
        self
    }

    pub fn with_reset_timeout(mut self, timeout: Duration) -> Self {
        self.reset_timeout = timeout;
        self
    }

    pub fn with_call_timeout(mut self, timeout: Duration) -> Self {
        self.call_timeout = timeout;
        self
    }
}

/// Internal state of the circuit breaker
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Closed,
    Open,
    HalfOpen,
}

/// Circuit breaker for external service calls
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: RwLock<State>,
    failure_count: AtomicU32,
    success_count: AtomicU32,
    last_failure_time: AtomicU64,
    last_state_change: AtomicU64,
    total_calls: AtomicU64,
    total_failures: AtomicU64,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(config: CircuitBreakerConfig) -> Self {
        let now = Instant::now().elapsed().as_millis() as u64;
        Self {
            config,
            state: RwLock::new(State::Closed),
            failure_count: AtomicU32::new(0),
            success_count: AtomicU32::new(0),
            last_failure_time: AtomicU64::new(0),
            last_state_change: AtomicU64::new(now),
            total_calls: AtomicU64::new(0),
            total_failures: AtomicU64::new(0),
        }
    }

    /// Check if the circuit is allowing calls
    pub async fn is_available(&self) -> bool {
        let state = *self.state.read().await;
        match state {
            State::Closed => true,
            State::HalfOpen => true, // Allow test calls
            State::Open => {
                // Check if reset timeout has passed
                let last_failure = self.last_failure_time.load(Ordering::Relaxed);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;

                if now - last_failure > self.config.reset_timeout.as_millis() as u64 {
                    // Transition to half-open
                    self.transition_to_half_open().await;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Execute a fallible operation through the circuit breaker
    pub async fn call<F, T, E>(&self, operation: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: std::future::Future<Output = Result<T, E>>,
    {
        self.total_calls.fetch_add(1, Ordering::Relaxed);

        // Check if circuit allows calls
        if !self.is_available().await {
            return Err(CircuitBreakerError::CircuitOpen);
        }

        // Execute with timeout
        let result = tokio::time::timeout(self.config.call_timeout, operation).await;

        match result {
            Ok(Ok(value)) => {
                self.record_success().await;
                Ok(value)
            }
            Ok(Err(e)) => {
                self.record_failure().await;
                Err(CircuitBreakerError::OperationFailed(e))
            }
            Err(_) => {
                self.record_failure().await;
                Err(CircuitBreakerError::Timeout)
            }
        }
    }

    /// Execute with a fallback value if circuit is open
    pub async fn call_with_fallback<F, T, E>(
        &self,
        operation: F,
        fallback: T,
    ) -> T
    where
        F: std::future::Future<Output = Result<T, E>>,
        T: Clone,
    {
        match self.call(operation).await {
            Ok(value) => value,
            Err(_) => fallback,
        }
    }

    /// Record a successful operation
    async fn record_success(&self) {
        let state = *self.state.read().await;

        match state {
            State::HalfOpen => {
                let successes = self.success_count.fetch_add(1, Ordering::SeqCst) + 1;
                if successes >= self.config.success_threshold {
                    self.transition_to_closed().await;
                }
            }
            State::Closed => {
                // Reset failure count on success
                self.failure_count.store(0, Ordering::Relaxed);
            }
            State::Open => {
                // Shouldn't happen, but handle gracefully
            }
        }
    }

    /// Record a failed operation
    async fn record_failure(&self) {
        self.total_failures.fetch_add(1, Ordering::Relaxed);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        self.last_failure_time.store(now, Ordering::Relaxed);

        let state = *self.state.read().await;

        match state {
            State::Closed => {
                let failures = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
                if failures >= self.config.failure_threshold {
                    self.transition_to_open().await;
                }
            }
            State::HalfOpen => {
                // Any failure in half-open returns to open
                self.transition_to_open().await;
            }
            State::Open => {
                // Already open
            }
        }
    }

    /// Transition to closed state
    async fn transition_to_closed(&self) {
        let mut state = self.state.write().await;
        if *state != State::Closed {
            *state = State::Closed;
            self.failure_count.store(0, Ordering::Relaxed);
            self.success_count.store(0, Ordering::Relaxed);

            metrics::set_circuit_breaker_state(&self.config.name, CircuitState::Closed);

            tracing::info!(
                circuit = %self.config.name,
                "Circuit breaker closed - service recovered"
            );
        }
    }

    /// Transition to open state
    async fn transition_to_open(&self) {
        let mut state = self.state.write().await;
        if *state != State::Open {
            *state = State::Open;
            self.success_count.store(0, Ordering::Relaxed);

            metrics::set_circuit_breaker_state(&self.config.name, CircuitState::Open);
            metrics::CIRCUIT_BREAKER_TRIPS
                .with_label_values(&[&self.config.name])
                .inc();

            tracing::warn!(
                circuit = %self.config.name,
                failures = self.failure_count.load(Ordering::Relaxed),
                reset_timeout_secs = self.config.reset_timeout.as_secs(),
                "Circuit breaker opened - service degraded"
            );
        }
    }

    /// Transition to half-open state
    async fn transition_to_half_open(&self) {
        let mut state = self.state.write().await;
        if *state == State::Open {
            *state = State::HalfOpen;
            self.success_count.store(0, Ordering::Relaxed);
            self.failure_count.store(0, Ordering::Relaxed);

            metrics::set_circuit_breaker_state(&self.config.name, CircuitState::HalfOpen);

            tracing::info!(
                circuit = %self.config.name,
                "Circuit breaker half-open - testing service"
            );
        }
    }

    /// Get current state
    pub async fn current_state(&self) -> &'static str {
        match *self.state.read().await {
            State::Closed => "closed",
            State::Open => "open",
            State::HalfOpen => "half-open",
        }
    }

    /// Get statistics
    pub fn stats(&self) -> CircuitBreakerStats {
        CircuitBreakerStats {
            name: self.config.name.clone(),
            total_calls: self.total_calls.load(Ordering::Relaxed),
            total_failures: self.total_failures.load(Ordering::Relaxed),
            current_failures: self.failure_count.load(Ordering::Relaxed),
            current_successes: self.success_count.load(Ordering::Relaxed),
        }
    }

    /// Manually reset the circuit breaker
    pub async fn reset(&self) {
        self.transition_to_closed().await;
    }

    /// Force the circuit open (for testing or manual intervention)
    pub async fn force_open(&self) {
        self.failure_count.store(self.config.failure_threshold, Ordering::Relaxed);
        self.transition_to_open().await;
    }
}

/// Circuit breaker statistics
#[derive(Debug, Clone)]
pub struct CircuitBreakerStats {
    pub name: String,
    pub total_calls: u64,
    pub total_failures: u64,
    pub current_failures: u32,
    pub current_successes: u32,
}

impl CircuitBreakerStats {
    pub fn failure_rate(&self) -> f64 {
        if self.total_calls == 0 {
            0.0
        } else {
            self.total_failures as f64 / self.total_calls as f64
        }
    }
}

/// Circuit breaker error types
#[derive(Debug)]
pub enum CircuitBreakerError<E> {
    /// Circuit is open, not accepting calls
    CircuitOpen,
    /// Operation timed out
    Timeout,
    /// Operation failed
    OperationFailed(E),
}

impl<E: std::fmt::Display> std::fmt::Display for CircuitBreakerError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CircuitOpen => write!(f, "Circuit breaker is open"),
            Self::Timeout => write!(f, "Operation timed out"),
            Self::OperationFailed(e) => write!(f, "Operation failed: {}", e),
        }
    }
}

impl<E: std::fmt::Debug + std::fmt::Display> std::error::Error for CircuitBreakerError<E> {}

/// Collection of circuit breakers for different services
pub struct CircuitBreakers {
    pub redis: Arc<CircuitBreaker>,
    pub postgres: Arc<CircuitBreaker>,
    pub cluster: Arc<CircuitBreaker>,
}

impl CircuitBreakers {
    /// Create default circuit breakers for all services
    pub fn new() -> Self {
        Self {
            redis: Arc::new(CircuitBreaker::new(
                CircuitBreakerConfig::new("redis")
                    .with_failure_threshold(3)
                    .with_reset_timeout(Duration::from_secs(10))
                    .with_call_timeout(Duration::from_secs(2)),
            )),
            postgres: Arc::new(CircuitBreaker::new(
                CircuitBreakerConfig::new("postgres")
                    .with_failure_threshold(5)
                    .with_reset_timeout(Duration::from_secs(30))
                    .with_call_timeout(Duration::from_secs(10)),
            )),
            cluster: Arc::new(CircuitBreaker::new(
                CircuitBreakerConfig::new("cluster")
                    .with_failure_threshold(3)
                    .with_reset_timeout(Duration::from_secs(15))
                    .with_call_timeout(Duration::from_secs(5)),
            )),
        }
    }

    /// Get health summary of all circuit breakers
    pub async fn health_summary(&self) -> CircuitBreakersHealth {
        CircuitBreakersHealth {
            redis: self.redis.current_state().await,
            postgres: self.postgres.current_state().await,
            cluster: self.cluster.current_state().await,
            redis_stats: self.redis.stats(),
            postgres_stats: self.postgres.stats(),
            cluster_stats: self.cluster.stats(),
        }
    }
}

impl Default for CircuitBreakers {
    fn default() -> Self {
        Self::new()
    }
}

/// Health summary for all circuit breakers
#[derive(Debug)]
pub struct CircuitBreakersHealth {
    pub redis: &'static str,
    pub postgres: &'static str,
    pub cluster: &'static str,
    pub redis_stats: CircuitBreakerStats,
    pub postgres_stats: CircuitBreakerStats,
    pub cluster_stats: CircuitBreakerStats,
}

impl CircuitBreakersHealth {
    pub fn all_healthy(&self) -> bool {
        self.redis == "closed" && self.postgres == "closed" && self.cluster == "closed"
    }

    pub fn any_open(&self) -> bool {
        self.redis == "open" || self.postgres == "open" || self.cluster == "open"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_opens_on_failures() {
        let cb = CircuitBreaker::new(
            CircuitBreakerConfig::new("test")
                .with_failure_threshold(3)
                .with_reset_timeout(Duration::from_millis(100)),
        );

        // Simulate failures
        for _ in 0..3 {
            let _: Result<(), CircuitBreakerError<&str>> = cb
                .call(async { Err::<(), _>("error") })
                .await;
        }

        // Circuit should be open
        assert!(!cb.is_available().await);
        assert_eq!(cb.current_state().await, "open");
    }

    #[tokio::test]
    async fn test_circuit_breaker_recovers() {
        let cb = CircuitBreaker::new(
            CircuitBreakerConfig::new("test")
                .with_failure_threshold(2)
                .with_success_threshold(2)
                .with_reset_timeout(Duration::from_millis(50)),
        );

        // Open the circuit
        for _ in 0..2 {
            let _: Result<(), CircuitBreakerError<&str>> = cb
                .call(async { Err::<(), _>("error") })
                .await;
        }

        assert_eq!(cb.current_state().await, "open");

        // Wait for reset timeout
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Should transition to half-open
        assert!(cb.is_available().await);

        // Successful calls should close the circuit
        for _ in 0..2 {
            let _: Result<i32, CircuitBreakerError<&str>> = cb
                .call(async { Ok::<i32, _>(42) })
                .await;
        }

        assert_eq!(cb.current_state().await, "closed");
    }
}
