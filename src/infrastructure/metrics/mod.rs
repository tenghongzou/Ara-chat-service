//! Prometheus metrics for production monitoring
//!
//! Provides comprehensive metrics for billion-scale chat service monitoring.

use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_gauge, register_gauge_vec, register_histogram_vec,
    register_int_counter, register_int_gauge, register_int_gauge_vec,
    CounterVec, Gauge, GaugeVec, HistogramVec, IntCounter, IntGauge, IntGaugeVec,
    Encoder, TextEncoder,
};
use std::time::Instant;

lazy_static! {
    // ========================================
    // Connection Metrics
    // ========================================

    /// Total WebSocket connections
    pub static ref WEBSOCKET_CONNECTIONS: IntGauge = register_int_gauge!(
        "chat_websocket_connections_total",
        "Total number of active WebSocket connections"
    ).unwrap();

    /// Connections by server/pod
    pub static ref CONNECTIONS_BY_SERVER: IntGaugeVec = register_int_gauge_vec!(
        "chat_connections_by_server",
        "Connections per server instance",
        &["server_id"]
    ).unwrap();

    /// Unique connected users
    pub static ref UNIQUE_USERS: IntGauge = register_int_gauge!(
        "chat_unique_users_total",
        "Total number of unique connected users"
    ).unwrap();

    /// Connection attempts
    pub static ref CONNECTION_ATTEMPTS: CounterVec = register_counter_vec!(
        "chat_connection_attempts_total",
        "Total connection attempts",
        &["status"] // success, auth_failed, rate_limited, error
    ).unwrap();

    /// Connection duration histogram
    pub static ref CONNECTION_DURATION: HistogramVec = register_histogram_vec!(
        "chat_connection_duration_seconds",
        "WebSocket connection duration",
        &["close_reason"],
        vec![1.0, 5.0, 30.0, 60.0, 300.0, 600.0, 1800.0, 3600.0]
    ).unwrap();

    // ========================================
    // Message Metrics
    // ========================================

    /// Messages sent by content type
    pub static ref MESSAGES_SENT: CounterVec = register_counter_vec!(
        "chat_messages_sent_total",
        "Total number of messages sent",
        &["content_type"] // text, image, file, system
    ).unwrap();

    /// Messages delivered by delivery type
    pub static ref MESSAGES_DELIVERED: CounterVec = register_counter_vec!(
        "chat_messages_delivered_total",
        "Total number of messages delivered",
        &["delivery_type"] // direct, broadcast, cluster_routed
    ).unwrap();

    /// Message processing duration
    pub static ref MESSAGE_PROCESSING_DURATION: HistogramVec = register_histogram_vec!(
        "chat_message_processing_duration_seconds",
        "Message processing duration in seconds",
        &["operation"], // send, store, route, deliver
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]
    ).unwrap();

    /// Message queue depth
    pub static ref MESSAGE_QUEUE_DEPTH: IntGaugeVec = register_int_gauge_vec!(
        "chat_message_queue_depth",
        "Current message queue depth",
        &["queue_type"] // inbound, outbound, offline
    ).unwrap();

    /// Failed message deliveries
    pub static ref MESSAGE_DELIVERY_FAILURES: CounterVec = register_counter_vec!(
        "chat_message_delivery_failures_total",
        "Failed message deliveries",
        &["reason"] // user_offline, timeout, error
    ).unwrap();

    // ========================================
    // Read Receipt & Reaction Metrics
    // ========================================

    /// Read receipts processed
    pub static ref READ_RECEIPTS: CounterVec = register_counter_vec!(
        "chat_read_receipts_total",
        "Total number of read receipts processed",
        &["status"] // success, error
    ).unwrap();

    /// Reactions processed
    pub static ref REACTIONS: CounterVec = register_counter_vec!(
        "chat_reactions_total",
        "Total number of reactions processed",
        &["action"] // add, remove
    ).unwrap();

    /// Unread counts updated
    pub static ref UNREAD_UPDATES: IntCounter = register_int_counter!(
        "chat_unread_updates_total",
        "Total unread count updates"
    ).unwrap();

    // ========================================
    // Cluster Metrics
    // ========================================

    /// Cluster routing
    pub static ref CLUSTER_MESSAGES_ROUTED: CounterVec = register_counter_vec!(
        "chat_cluster_messages_routed_total",
        "Total number of messages routed across cluster",
        &["target"] // local, remote, broadcast
    ).unwrap();

    /// Cluster node health
    pub static ref CLUSTER_NODE_HEALTH: GaugeVec = register_gauge_vec!(
        "chat_cluster_node_health",
        "Cluster node health status (1=healthy, 0=unhealthy)",
        &["node_id"]
    ).unwrap();

    /// Active cluster sessions
    pub static ref CLUSTER_SESSIONS: IntGauge = register_int_gauge!(
        "chat_cluster_sessions_total",
        "Total active cluster sessions"
    ).unwrap();

    /// Cluster subscription status (1=subscribed, 0=disconnected)
    pub static ref CLUSTER_SUBSCRIBED: IntGauge = register_int_gauge!(
        "chat_cluster_subscribed",
        "Cluster pub/sub subscription status"
    ).unwrap();

    /// Messages received from other cluster nodes
    pub static ref CLUSTER_MESSAGES_RECEIVED: IntCounter = register_int_counter!(
        "chat_cluster_messages_received_total",
        "Total messages received from other cluster nodes"
    ).unwrap();

    // ========================================
    // Database Metrics
    // ========================================

    /// Database query duration
    pub static ref DB_QUERY_DURATION: HistogramVec = register_histogram_vec!(
        "chat_db_query_duration_seconds",
        "Database query duration",
        &["operation", "table"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]
    ).unwrap();

    /// Database connection pool stats
    pub static ref DB_POOL_SIZE: IntGaugeVec = register_int_gauge_vec!(
        "chat_db_pool_size",
        "Database connection pool size",
        &["pool", "state"] // active, idle
    ).unwrap();

    /// Database errors
    pub static ref DB_ERRORS: CounterVec = register_counter_vec!(
        "chat_db_errors_total",
        "Database errors",
        &["operation", "error_type"]
    ).unwrap();

    // ========================================
    // Redis Metrics
    // ========================================

    /// Redis operation duration
    pub static ref REDIS_OPERATION_DURATION: HistogramVec = register_histogram_vec!(
        "chat_redis_operation_duration_seconds",
        "Redis operation duration",
        &["operation"],
        vec![0.0005, 0.001, 0.005, 0.01, 0.025, 0.05, 0.1]
    ).unwrap();

    /// Redis cache hits/misses
    pub static ref REDIS_CACHE: CounterVec = register_counter_vec!(
        "chat_redis_cache_total",
        "Redis cache operations",
        &["operation", "result"] // get/hit, get/miss, set/success
    ).unwrap();

    /// Redis pub/sub messages
    pub static ref REDIS_PUBSUB: CounterVec = register_counter_vec!(
        "chat_redis_pubsub_total",
        "Redis pub/sub messages",
        &["direction"] // publish, subscribe
    ).unwrap();

    // ========================================
    // Rate Limiting Metrics
    // ========================================

    /// Rate limit checks
    pub static ref RATE_LIMIT_CHECKS: CounterVec = register_counter_vec!(
        "chat_rate_limit_checks_total",
        "Rate limit checks performed",
        &["result"] // allowed, denied
    ).unwrap();

    /// Current rate limit usage
    pub static ref RATE_LIMIT_USAGE: GaugeVec = register_gauge_vec!(
        "chat_rate_limit_usage_ratio",
        "Current rate limit usage ratio",
        &["limit_type"] // connection, message, api
    ).unwrap();

    // ========================================
    // Circuit Breaker Metrics
    // ========================================

    /// Circuit breaker state
    pub static ref CIRCUIT_BREAKER_STATE: IntGaugeVec = register_int_gauge_vec!(
        "chat_circuit_breaker_state",
        "Circuit breaker state (0=closed, 1=half-open, 2=open)",
        &["service"]
    ).unwrap();

    /// Circuit breaker trips
    pub static ref CIRCUIT_BREAKER_TRIPS: CounterVec = register_counter_vec!(
        "chat_circuit_breaker_trips_total",
        "Circuit breaker trip count",
        &["service"]
    ).unwrap();

    // ========================================
    // Presence Metrics
    // ========================================

    /// Online users by status
    pub static ref PRESENCE_BY_STATUS: IntGaugeVec = register_int_gauge_vec!(
        "chat_presence_by_status",
        "Users by presence status",
        &["status"] // online, away, busy, offline
    ).unwrap();

    /// Presence updates
    pub static ref PRESENCE_UPDATES: IntCounter = register_int_counter!(
        "chat_presence_updates_total",
        "Total presence updates"
    ).unwrap();

    // ========================================
    // Storage/Maintenance Metrics
    // ========================================

    /// Total messages stored
    pub static ref MESSAGES_STORED: IntCounter = register_int_counter!(
        "chat_messages_stored_total",
        "Total messages stored (permanent storage)"
    ).unwrap();

    /// Last partition management timestamp
    pub static ref LAST_PARTITION_MGMT_TIMESTAMP: IntGauge = register_int_gauge!(
        "chat_last_partition_mgmt_timestamp_seconds",
        "Unix timestamp of last partition management run"
    ).unwrap();

    /// Partition management duration
    pub static ref PARTITION_MGMT_DURATION: Gauge = register_gauge!(
        "chat_partition_mgmt_duration_seconds",
        "Duration of last partition management operation"
    ).unwrap();

    // ========================================
    // System Metrics
    // ========================================

    /// Server uptime
    pub static ref SERVER_UPTIME: Gauge = register_gauge!(
        "chat_server_uptime_seconds",
        "Server uptime in seconds"
    ).unwrap();

    /// Memory usage (if available)
    pub static ref MEMORY_USAGE: Gauge = register_gauge!(
        "chat_memory_usage_bytes",
        "Current memory usage in bytes"
    ).unwrap();

    /// Active goroutines/tasks
    pub static ref ACTIVE_TASKS: IntGauge = register_int_gauge!(
        "chat_active_tasks",
        "Number of active background tasks"
    ).unwrap();
}

// ========================================
// Metric Helper Functions
// ========================================

/// Update connection metrics
pub fn update_connection_metrics(connections: usize, users: usize) {
    WEBSOCKET_CONNECTIONS.set(connections as i64);
    UNIQUE_USERS.set(users as i64);
}

/// Record a connection attempt
pub fn record_connection_attempt(success: bool, reason: &str) {
    let status = if success { "success" } else { reason };
    CONNECTION_ATTEMPTS.with_label_values(&[status]).inc();
}

/// Record connection close with duration
pub fn record_connection_close(duration_secs: f64, reason: &str) {
    CONNECTION_DURATION.with_label_values(&[reason]).observe(duration_secs);
}

/// Record a sent message
pub fn record_message_sent(content_type: &str) {
    MESSAGES_SENT.with_label_values(&[content_type]).inc();
}

/// Record a delivered message
pub fn record_message_delivered(delivery_type: &str) {
    MESSAGES_DELIVERED.with_label_values(&[delivery_type]).inc();
}

/// Record message processing with timing
pub fn record_message_processing(operation: &str, duration_secs: f64) {
    MESSAGE_PROCESSING_DURATION
        .with_label_values(&[operation])
        .observe(duration_secs);
}

/// Record a database query
pub fn record_db_query(operation: &str, table: &str, duration_secs: f64) {
    DB_QUERY_DURATION
        .with_label_values(&[operation, table])
        .observe(duration_secs);
}

/// Record database error
pub fn record_db_error(operation: &str, error_type: &str) {
    DB_ERRORS.with_label_values(&[operation, error_type]).inc();
}

/// Record Redis operation
pub fn record_redis_operation(operation: &str, duration_secs: f64) {
    REDIS_OPERATION_DURATION
        .with_label_values(&[operation])
        .observe(duration_secs);
}

/// Record cache hit/miss
pub fn record_cache_result(hit: bool) {
    let result = if hit { "hit" } else { "miss" };
    REDIS_CACHE.with_label_values(&["get", result]).inc();
}

/// Record rate limit check
pub fn record_rate_limit_check(allowed: bool) {
    let result = if allowed { "allowed" } else { "denied" };
    RATE_LIMIT_CHECKS.with_label_values(&[result]).inc();
}

/// Set circuit breaker state
pub fn set_circuit_breaker_state(service: &str, state: CircuitState) {
    CIRCUIT_BREAKER_STATE
        .with_label_values(&[service])
        .set(state as i64);
}

/// Circuit breaker states
#[derive(Clone, Copy)]
pub enum CircuitState {
    Closed = 0,
    HalfOpen = 1,
    Open = 2,
}

/// Record partition management operation
pub fn record_partition_management(duration_secs: f64) {
    PARTITION_MGMT_DURATION.set(duration_secs);
    LAST_PARTITION_MGMT_TIMESTAMP.set(chrono::Utc::now().timestamp());
}

/// Record message stored
pub fn record_message_stored() {
    MESSAGES_STORED.inc();
}

/// Timer for measuring operation duration
pub struct MetricTimer {
    start: Instant,
    metric: &'static HistogramVec,
    labels: Vec<String>,
}

impl MetricTimer {
    pub fn new(metric: &'static HistogramVec, labels: Vec<&str>) -> Self {
        Self {
            start: Instant::now(),
            metric,
            labels: labels.into_iter().map(String::from).collect(),
        }
    }

    pub fn observe(self) {
        let duration = self.start.elapsed().as_secs_f64();
        let label_refs: Vec<&str> = self.labels.iter().map(|s| s.as_str()).collect();
        self.metric.with_label_values(&label_refs).observe(duration);
    }
}

/// Get all metrics as Prometheus text format
pub fn gather_metrics() -> String {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}
