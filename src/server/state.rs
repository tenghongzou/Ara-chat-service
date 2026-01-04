//! Application state

use std::sync::Arc;
use std::time::Instant;

use sqlx::PgPool;

use crate::attachment::AttachmentService;
use crate::auth::{JwtError, JwtValidator};
use crate::blocking::BlockingService;
use crate::circuit_breaker::CircuitBreakers;
use crate::cluster::{ClusterRouter, MemorySessionStore, RedisSessionStore, SessionStore};
use crate::config::Settings;
use crate::connection::{ConnectionLimits, ConnectionManager};
use crate::conversation::ConversationService;
use crate::gdpr::{GdprService, GdprServiceConfig};
use crate::link_preview::LinkPreviewService;
use crate::message::{MessageHandler, MessageRouter, MessageStorage, OfflineQueue};
use crate::notification::{NotificationPublisher, NotificationPublisherConfig};
use crate::postgres::PostgresPool;
use crate::presence::{PresenceTracker, PresenceBroadcaster};
use crate::ratelimit::RateLimiter;
use crate::reaction::ReactionService;
use crate::receipt::ReadReceiptTracker;
use crate::redis::{RedisCache, RedisFallback, RedisPool};

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
    pub attachment_service: Option<Arc<AttachmentService>>,
    pub notification_publisher: Option<Arc<NotificationPublisher>>,
    pub gdpr_service: Option<Arc<GdprService>>,
    pub blocking_service: Option<Arc<BlockingService>>,
    pub link_preview_service: Option<Arc<LinkPreviewService>>,
    pub start_time: Instant,
}

impl AppState {
    /// Create a minimal AppState for testing (no external dependencies)
    #[cfg(any(test, feature = "test-utils"))]
    pub fn for_testing(settings: Settings) -> Result<Self, JwtError> {
        let jwt_validator = Arc::new(JwtValidator::new(&(&settings.jwt).into())?);

        let limits = ConnectionLimits {
            max_connections: settings.websocket.max_connections,
            max_connections_per_user: settings.websocket.max_connections_per_user,
        };
        let connection_manager = Arc::new(ConnectionManager::with_limits(limits));
        let rate_limiter = Arc::new(RateLimiter::new(None));
        let offline_queue = Arc::new(OfflineQueue::new(None));

        Ok(Self {
            settings: Arc::new(settings),
            jwt_validator,
            connection_manager,
            redis_pool: None,
            postgres_pool: None,
            session_store: None,
            cluster_router: None,
            presence_tracker: None,
            presence_broadcaster: None,
            message_handler: None,
            message_storage: None,
            conversation_service: None,
            receipt_tracker: None,
            reaction_service: None,
            redis_cache: None,
            rate_limiter,
            offline_queue,
            circuit_breakers: None,
            attachment_service: None,
            notification_publisher: None,
            gdpr_service: None,
            blocking_service: None,
            link_preview_service: None,
            start_time: Instant::now(),
        })
    }

    pub async fn new(settings: Settings) -> Result<Self, JwtError> {
        let jwt_validator = Arc::new(JwtValidator::new(&(&settings.jwt).into())?);

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
        let cluster_router = if let Some(ref store) = session_store {
            let router = if let Some(ref pool) = redis_pool {
                ClusterRouter::with_redis(
                    connection_manager.clone(),
                    store.clone(),
                    pool.clone(),
                    settings.cluster.server_id.clone(),
                )
            } else {
                ClusterRouter::new(
                    connection_manager.clone(),
                    store.clone(),
                    settings.cluster.server_id.clone(),
                )
            };
            Some(Arc::new(router.with_offline_queue(offline_queue.clone())))
        } else {
            None
        };

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

        // Create notification publisher (if enabled)
        // Must be created before MessageRouter so it can be injected
        let notification_publisher = if settings.notification.enabled {
            let redis_fallback = Arc::new(RedisFallback::new(redis_pool.clone()));
            let config = NotificationPublisherConfig {
                enabled: settings.notification.enabled,
                ttl_seconds: settings.notification.ttl_seconds,
                notify_new_messages: settings.notification.notify_new_messages,
                notify_mentions: settings.notification.notify_mentions,
                notify_reactions: settings.notification.notify_reactions,
            };
            tracing::info!("Notification publisher initialized");
            Some(Arc::new(NotificationPublisher::new(redis_fallback, config)))
        } else {
            tracing::info!("Notification publisher disabled");
            None
        };

        // Create blocking service (only if PostgreSQL is available)
        // Must be created before MessageRouter so it can be injected
        // TODO: Add tenant_id to Settings when multi-tenancy is implemented
        let blocking_service = if let Some(ref pg_pool) = postgres_pool {
            let sqlx_pool: Arc<PgPool> = Arc::new(pg_pool.pool().clone());
            tracing::info!("Blocking service initialized");
            Some(Arc::new(BlockingService::new(
                sqlx_pool,
                "default".to_string(),
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

                // Message handler (works with or without cluster router)
                let handler = {
                    let mut router = MessageRouter::new(
                        connection_manager.clone(),
                        cluster_router.clone(),
                        conv_service.clone(),
                    );
                    // Inject notification publisher for offline user notifications
                    if let Some(ref publisher) = notification_publisher {
                        router = router.with_notification_publisher(publisher.clone());
                    }
                    // Inject blocking service for filtering blocked users
                    if let Some(ref blocking) = blocking_service {
                        router = router.with_blocking_service(blocking.clone());
                    }
                    Some(Arc::new(MessageHandler::new(
                        storage.clone(),
                        Arc::new(router),
                        conv_service.clone(),
                    )))
                };

                (Some(storage), Some(conv_service), handler, Some(receipt), Some(reaction))
            } else {
                (None, None, None, None, None)
            };

        // Create attachment service (only if PostgreSQL and conversation service are available)
        let attachment_service = if let (Some(ref pg_pool), Some(ref conv_service)) =
            (&postgres_pool, &conversation_service)
        {
            let sqlx_pool: Arc<PgPool> = Arc::new(pg_pool.pool().clone());
            match AttachmentService::from_settings(
                sqlx_pool,
                settings.file_storage.clone(),
                conv_service.clone(),
            )
            .await
            {
                Ok(service) => {
                    tracing::info!("Attachment service initialized");
                    Some(Arc::new(service))
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to initialize attachment service");
                    None
                }
            }
        } else {
            None
        };

        // Create GDPR service (only if PostgreSQL is available and GDPR is enabled)
        let gdpr_service = if let Some(ref pg_pool) = postgres_pool {
            if settings.gdpr.enabled {
                let sqlx_pool: Arc<PgPool> = Arc::new(pg_pool.pool().clone());
                let config = GdprServiceConfig {
                    enabled: settings.gdpr.enabled,
                    export_path: std::path::PathBuf::from(&settings.gdpr.export_path),
                    export_retention_days: settings.gdpr.export_retention_days,
                    audit_log_retention_years: settings.gdpr.audit_log_retention_years,
                };
                tracing::info!(
                    export_path = %settings.gdpr.export_path,
                    "GDPR service initialized"
                );
                Some(Arc::new(GdprService::new(sqlx_pool, config)))
            } else {
                tracing::info!("GDPR service disabled");
                None
            }
        } else {
            None
        };

        // Create link preview service (only if PostgreSQL is available)
        let link_preview_service = if let Some(ref pg_pool) = postgres_pool {
            let sqlx_pool: Arc<PgPool> = Arc::new(pg_pool.pool().clone());
            tracing::info!("Link preview service initialized");
            Some(Arc::new(LinkPreviewService::new(sqlx_pool, redis_pool.clone())))
        } else {
            None
        };

        Ok(Self {
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
            attachment_service,
            notification_publisher,
            gdpr_service,
            blocking_service,
            link_preview_service,
            start_time: Instant::now(),
        })
    }
}
