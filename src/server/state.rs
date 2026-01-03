//! Application state

use std::sync::Arc;
use std::time::Instant;

use sqlx::PgPool;

use crate::auth::JwtValidator;
use crate::circuit_breaker::CircuitBreakers;
use crate::cluster::{ClusterRouter, MemorySessionStore, RedisSessionStore, SessionStore};
use crate::config::Settings;
use crate::connection::{ConnectionLimits, ConnectionManager};
use crate::conversation::ConversationService;
use crate::message::{MessageHandler, MessageRouter, MessageStorage, OfflineQueue};
use crate::postgres::PostgresPool;
use crate::presence::{PresenceTracker, PresenceBroadcaster};
use crate::ratelimit::RateLimiter;
use crate::reaction::ReactionService;
use crate::receipt::ReadReceiptTracker;
use crate::redis::{RedisCache, RedisPool};

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub jwt_validator: Arc<JwtValidator>,
    pub connection_manager: Arc<ConnectionManager>,
    pub redis_pool: Option<Arc<RedisPool>>,
    pub postgres_pool: Option<Arc<PostgresPool>>,
    pub session_store: Option<Arc<dyn SessionStore>>,
    pub cluster_router: Option<Arc<ClusterRouter>>,
    pub presence_tracker: Option<Arc<PresenceTracker>>,
    pub presence_broadcaster: Option<Arc<PresenceBroadcaster>>,
    pub message_handler: Option<Arc<MessageHandler>>,
    pub message_storage: Option<Arc<MessageStorage>>,
    pub conversation_service: Option<Arc<ConversationService>>,
    pub receipt_tracker: Option<Arc<ReadReceiptTracker>>,
    pub reaction_service: Option<Arc<ReactionService>>,
    pub redis_cache: Option<Arc<RedisCache>>,
    pub rate_limiter: Arc<RateLimiter>,
    pub offline_queue: Arc<OfflineQueue>,
    pub circuit_breakers: Option<Arc<CircuitBreakers>>,
    pub start_time: Instant,
}

impl AppState {
    pub async fn new(settings: Settings) -> Self {
        let jwt_validator = Arc::new(JwtValidator::new(&(&settings.jwt).into()));

        // Create connection manager with limits
        let limits = ConnectionLimits {
            max_connections: settings.websocket.max_connections,
            max_connections_per_user: settings.websocket.max_connections_per_user,
        };
        let connection_manager = Arc::new(ConnectionManager::with_limits(limits));

        // Create Redis pool if enabled
        let redis_pool = match RedisPool::new(&settings.redis) {
            Ok(pool) => {
                tracing::info!(url = %settings.redis.url, "Redis pool created");
                Some(Arc::new(pool))
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to create Redis pool");
                None
            }
        };

        // Create PostgreSQL pool with optional migrations
        let postgres_pool = match PostgresPool::new_with_migrations(
            &settings.database,
            &settings.database.migrations_path,
        ).await {
            Ok(pool) => {
                tracing::info!("PostgreSQL pool created");
                Some(Arc::new(pool))
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to create PostgreSQL pool");
                None
            }
        };

        // Create session store
        let session_store: Option<Arc<dyn SessionStore>> = if settings.cluster.enabled {
            if let Some(ref pool) = redis_pool {
                Some(Arc::new(RedisSessionStore::new(
                    settings.cluster.server_id.clone(),
                    pool.clone(),
                )))
            } else {
                tracing::warn!("Cluster mode enabled but Redis not available, using memory session store");
                Some(Arc::new(MemorySessionStore::new(settings.cluster.server_id.clone())))
            }
        } else {
            None
        };

        // Create Redis cache
        let redis_cache = redis_pool.as_ref().map(|pool| {
            Arc::new(RedisCache::new(pool.clone()))
        });

        // Create rate limiter
        let rate_limiter = Arc::new(RateLimiter::new(redis_pool.clone()));

        // Create offline message queue
        let offline_queue = Arc::new(OfflineQueue::new(redis_pool.clone()));

        // Create circuit breakers for external services
        let circuit_breakers = Some(Arc::new(CircuitBreakers::new()));

        // Create cluster router (with offline queue for message delivery to offline users)
        let cluster_router = session_store.as_ref().map(|store| {
            Arc::new(
                ClusterRouter::new(
                    connection_manager.clone(),
                    store.clone(),
                    settings.cluster.server_id.clone(),
                ).with_offline_queue(offline_queue.clone())
            )
        });

        // Create presence tracker
        let presence_tracker = Some(Arc::new(PresenceTracker::new(
            redis_pool.clone(),
            settings.cluster.server_id.clone(),
        )));

        // Create presence broadcaster (needs presence_tracker and cluster_router)
        let presence_broadcaster = if let (Some(ref tracker), Some(ref router)) =
            (&presence_tracker, &cluster_router)
        {
            Some(Arc::new(PresenceBroadcaster::new(
                connection_manager.clone(),
                router.clone(),
                tracker.clone(),
            )))
        } else {
            None
        };

        // Create domain services (only if PostgreSQL is available)
        let (message_storage, conversation_service, message_handler, receipt_tracker, reaction_service) =
            if let Some(ref pg_pool) = postgres_pool {
                let sqlx_pool: Arc<PgPool> = Arc::new(pg_pool.pool().clone());

                // Message storage
                let storage = Arc::new(MessageStorage::new(sqlx_pool.clone()));

                // Conversation service
                let conv_service = Arc::new(ConversationService::new(sqlx_pool.clone()));

                // Receipt tracker with Redis cache
                let receipt = Arc::new(ReadReceiptTracker::new(
                    sqlx_pool.clone(),
                    redis_pool.clone(),
                ));

                // Reaction service
                let reaction = Arc::new(ReactionService::new(sqlx_pool.clone()));

                // Message handler (needs cluster router)
                let handler = if let Some(ref cr) = cluster_router {
                    let router = Arc::new(MessageRouter::new(
                        connection_manager.clone(),
                        cr.clone(),
                        conv_service.clone(),
                    ));
                    Some(Arc::new(MessageHandler::new(
                        storage.clone(),
                        router,
                        conv_service.clone(),
                    )))
                } else {
                    None
                };

                (Some(storage), Some(conv_service), handler, Some(receipt), Some(reaction))
            } else {
                (None, None, None, None, None)
            };

        Self {
            settings: Arc::new(settings),
            jwt_validator,
            connection_manager,
            redis_pool,
            postgres_pool,
            session_store,
            cluster_router,
            presence_tracker,
            presence_broadcaster,
            message_handler,
            message_storage,
            conversation_service,
            receipt_tracker,
            reaction_service,
            redis_cache,
            rate_limiter,
            offline_queue,
            circuit_breakers,
            start_time: Instant::now(),
        }
    }
}
