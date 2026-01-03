//! Redis connection pool and utilities

mod pool;
mod cache;
mod cluster;
mod fallback;

pub use pool::RedisPool;
pub use cache::{RedisCache, CacheError};
pub use cluster::{
    RedisClusterPool, RedisClusterConfig, RedisClusterError,
    ShardedRedisOps, UserRedisOps, ClusterHealth, ClusterState,
};
pub use fallback::{RedisFallback, FallbackConfig};
