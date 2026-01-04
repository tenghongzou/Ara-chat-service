//! Link Preview service
//!
//! Handles fetching, caching, and storing link preview metadata.

use std::sync::Arc;
use std::time::Duration;

use redis::AsyncCommands;
use reqwest::Client;
use sqlx::PgPool;
use uuid::Uuid;

use super::error::LinkPreviewError;
use super::parser::{extract_urls, parse_open_graph, url_hash};
use super::types::{LinkPreview, LinkPreviewRow, OpenGraphData, PendingPreview};
use crate::redis::RedisPool;

/// Default fetch timeout in seconds
const DEFAULT_FETCH_TIMEOUT_SECS: u64 = 5;

/// Default cache TTL in seconds (24 hours)
const DEFAULT_CACHE_TTL_SECS: i64 = 24 * 60 * 60;

/// Maximum content length to fetch (1MB)
const MAX_CONTENT_LENGTH: usize = 1_048_576;

/// Maximum number of pending previews to process per batch
const MAX_BATCH_SIZE: usize = 50;

/// Redis cache key prefix
const CACHE_PREFIX: &str = "chat:linkpreview";

/// Link Preview service for fetching and storing Open Graph metadata
#[derive(Clone)]
pub struct LinkPreviewService {
    pool: Arc<PgPool>,
    redis: Option<Arc<RedisPool>>,
    http_client: Client,
    fetch_timeout: Duration,
    cache_ttl: i64,
}

impl LinkPreviewService {
    /// Create a new LinkPreviewService
    pub fn new(pool: Arc<PgPool>, redis: Option<Arc<RedisPool>>) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_FETCH_TIMEOUT_SECS))
            .user_agent("AraBot/1.0 (+https://ara.app)")
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            pool,
            redis,
            http_client,
            fetch_timeout: Duration::from_secs(DEFAULT_FETCH_TIMEOUT_SECS),
            cache_ttl: DEFAULT_CACHE_TTL_SECS,
        }
    }

    /// Create with custom settings
    pub fn with_settings(
        pool: Arc<PgPool>,
        redis: Option<Arc<RedisPool>>,
        fetch_timeout_secs: u64,
        cache_ttl_hours: u64,
    ) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(fetch_timeout_secs))
            .user_agent("AraBot/1.0 (+https://ara.app)")
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            pool,
            redis,
            http_client,
            fetch_timeout: Duration::from_secs(fetch_timeout_secs),
            cache_ttl: (cache_ttl_hours * 60 * 60) as i64,
        }
    }

    /// Extract URLs from message content and enqueue them for preview fetching
    pub async fn enqueue_previews(
        &self,
        message_id: Uuid,
        content: &str,
    ) -> Result<usize, LinkPreviewError> {
        let urls = extract_urls(content);
        if urls.is_empty() {
            return Ok(0);
        }

        let mut count = 0;
        for url in urls {
            let hash = url_hash(&url);

            // Insert pending preview record
            let result = sqlx::query(
                r#"
                INSERT INTO link_previews (message_id, url, url_hash, status)
                VALUES ($1, $2, $3, 'pending')
                ON CONFLICT (message_id, url_hash) DO NOTHING
                "#,
            )
            .bind(message_id)
            .bind(&url)
            .bind(&hash)
            .execute(self.pool.as_ref())
            .await?;

            if result.rows_affected() > 0 {
                count += 1;
            }
        }

        if count > 0 {
            tracing::debug!(
                message_id = %message_id,
                url_count = count,
                "Enqueued link previews"
            );
        }

        Ok(count)
    }

    /// Fetch Open Graph metadata for a URL
    pub async fn fetch_preview(&self, url: &str) -> Result<OpenGraphData, LinkPreviewError> {
        // Check cache first
        if let Some(cached) = self.get_cached(url).await? {
            return Ok(cached);
        }

        // Fetch the URL
        let response = tokio::time::timeout(
            self.fetch_timeout,
            self.http_client.get(url).send(),
        )
        .await
        .map_err(|_| LinkPreviewError::Timeout)?
        .map_err(LinkPreviewError::Http)?;

        // Check content length
        if let Some(content_length) = response.content_length() {
            if content_length as usize > MAX_CONTENT_LENGTH {
                return Err(LinkPreviewError::ContentTooLarge {
                    size: content_length as usize,
                    max: MAX_CONTENT_LENGTH,
                });
            }
        }

        // Get HTML content
        let html = response.text().await.map_err(LinkPreviewError::Http)?;

        // Check actual content length
        if html.len() > MAX_CONTENT_LENGTH {
            return Err(LinkPreviewError::ContentTooLarge {
                size: html.len(),
                max: MAX_CONTENT_LENGTH,
            });
        }

        // Parse Open Graph metadata
        let data = parse_open_graph(&html, url);

        // Cache the result
        self.set_cached(url, &data).await?;

        Ok(data)
    }

    /// Get previews for a message
    pub async fn get_previews_for_message(
        &self,
        message_id: Uuid,
    ) -> Result<Vec<LinkPreview>, LinkPreviewError> {
        let rows: Vec<LinkPreviewRow> = sqlx::query_as::<_, LinkPreviewRow>(
            r#"
            SELECT id, message_id, url, url_hash, title, description, image_url,
                   site_name, favicon_url, status, error, fetched_at, created_at
            FROM link_previews
            WHERE message_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(message_id)
        .fetch_all(self.pool.as_ref())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_link_preview()).collect())
    }

    /// Get pending previews for background processing
    pub async fn get_pending_previews(&self) -> Result<Vec<PendingPreview>, LinkPreviewError> {
        let rows: Vec<PendingPreview> = sqlx::query_as::<_, PendingPreview>(
            r#"
            SELECT id, message_id, url, url_hash, created_at
            FROM link_previews
            WHERE status = 'pending'
            ORDER BY created_at ASC
            LIMIT $1
            "#,
        )
        .bind(MAX_BATCH_SIZE as i32)
        .fetch_all(self.pool.as_ref())
        .await?;

        Ok(rows)
    }

    /// Process a single pending preview
    pub async fn process_preview(&self, preview: &PendingPreview) -> Result<LinkPreview, LinkPreviewError> {
        let result = self.fetch_preview(&preview.url).await;

        match result {
            Ok(data) => {
                // Update with success
                let row: LinkPreviewRow = sqlx::query_as::<_, LinkPreviewRow>(
                    r#"
                    UPDATE link_previews
                    SET status = 'success',
                        title = $2,
                        description = $3,
                        image_url = $4,
                        site_name = $5,
                        favicon_url = $6,
                        fetched_at = NOW(),
                        error = NULL
                    WHERE id = $1
                    RETURNING id, message_id, url, url_hash, title, description, image_url,
                              site_name, favicon_url, status, error, fetched_at, created_at
                    "#,
                )
                .bind(preview.id)
                .bind(&data.title)
                .bind(&data.description)
                .bind(&data.image)
                .bind(&data.site_name)
                .bind(&data.favicon)
                .fetch_one(self.pool.as_ref())
                .await?;

                Ok(row.into_link_preview())
            }
            Err(e) => {
                // Update with failure
                let error_msg = e.to_string();
                let row: LinkPreviewRow = sqlx::query_as::<_, LinkPreviewRow>(
                    r#"
                    UPDATE link_previews
                    SET status = 'failed',
                        error = $2,
                        fetched_at = NOW()
                    WHERE id = $1
                    RETURNING id, message_id, url, url_hash, title, description, image_url,
                              site_name, favicon_url, status, error, fetched_at, created_at
                    "#,
                )
                .bind(preview.id)
                .bind(&error_msg)
                .fetch_one(self.pool.as_ref())
                .await?;

                tracing::warn!(
                    preview_id = %preview.id,
                    url = %preview.url,
                    error = %error_msg,
                    "Failed to fetch link preview"
                );

                Ok(row.into_link_preview())
            }
        }
    }

    /// Process all pending previews
    /// Returns the number of previews processed
    pub async fn process_pending_previews(&self) -> Result<usize, LinkPreviewError> {
        let pending = self.get_pending_previews().await?;
        let count = pending.len();

        for preview in pending {
            // Process each preview, but don't fail the batch if one fails
            if let Err(e) = self.process_preview(&preview).await {
                tracing::error!(
                    preview_id = %preview.id,
                    error = %e,
                    "Error processing preview"
                );
            }
        }

        Ok(count)
    }

    /// Get conversation ID for a preview (needed for broadcasting)
    pub async fn get_conversation_id(&self, message_id: Uuid) -> Result<Option<Uuid>, LinkPreviewError> {
        let row: Option<(Uuid,)> = sqlx::query_as(
            r#"
            SELECT conversation_id
            FROM messages
            WHERE id = $1
            "#,
        )
        .bind(message_id)
        .fetch_optional(self.pool.as_ref())
        .await?;

        Ok(row.map(|(id,)| id))
    }

    /// Refresh failed previews for a message
    pub async fn refresh_failed_previews(&self, message_id: Uuid) -> Result<usize, LinkPreviewError> {
        let result = sqlx::query(
            r#"
            UPDATE link_previews
            SET status = 'pending', error = NULL, fetched_at = NULL
            WHERE message_id = $1 AND status = 'failed'
            "#,
        )
        .bind(message_id)
        .execute(self.pool.as_ref())
        .await?;

        Ok(result.rows_affected() as usize)
    }

    // ==================== Redis Cache Methods ====================

    fn cache_key(&self, url: &str) -> String {
        let hash = url_hash(url);
        format!("{}:{}", CACHE_PREFIX, hash)
    }

    async fn get_cached(&self, url: &str) -> Result<Option<OpenGraphData>, LinkPreviewError> {
        let Some(ref redis) = self.redis else {
            return Ok(None);
        };

        let mut conn = redis
            .get_connection()
            .await
            .map_err(|e| LinkPreviewError::Redis(e.to_string()))?;

        let key = self.cache_key(url);
        let json: Option<String> = conn.get(&key).await.ok();

        match json {
            Some(json) => {
                let og: OpenGraphData = serde_json::from_str(&json)
                    .map_err(|e| LinkPreviewError::Redis(e.to_string()))?;
                Ok(Some(og))
            }
            None => Ok(None),
        }
    }

    async fn set_cached(&self, url: &str, data: &OpenGraphData) -> Result<(), LinkPreviewError> {
        let Some(ref redis) = self.redis else {
            return Ok(());
        };

        let mut conn = redis
            .get_connection()
            .await
            .map_err(|e| LinkPreviewError::Redis(e.to_string()))?;

        let key = self.cache_key(url);
        let json = serde_json::to_string(data)
            .map_err(|e| LinkPreviewError::Redis(e.to_string()))?;

        let _: () = conn
            .set_ex(&key, &json, self.cache_ttl as u64)
            .await
            .map_err(|e| LinkPreviewError::Redis(e.to_string()))?;

        Ok(())
    }
}

// ==================== SQLx FromRow Implementations ====================

impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for LinkPreviewRow {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Self {
            id: row.try_get("id")?,
            message_id: row.try_get("message_id")?,
            url: row.try_get("url")?,
            url_hash: row.try_get("url_hash")?,
            title: row.try_get("title")?,
            description: row.try_get("description")?,
            image_url: row.try_get("image_url")?,
            site_name: row.try_get("site_name")?,
            favicon_url: row.try_get("favicon_url")?,
            status: row.try_get("status")?,
            error: row.try_get("error")?,
            fetched_at: row.try_get("fetched_at")?,
            created_at: row.try_get("created_at")?,
        })
    }
}

impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for PendingPreview {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Self {
            id: row.try_get("id")?,
            message_id: row.try_get("message_id")?,
            url: row.try_get("url")?,
            url_hash: row.try_get("url_hash")?,
            created_at: row.try_get("created_at")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::parser::url_hash;
    use super::super::types::PreviewStatus;

    #[test]
    fn test_cache_key_format() {
        // Test that cache key uses the correct prefix and hash format
        let url = "https://example.com";
        let hash = url_hash(url);
        let key = format!("chat:linkpreview:{}", hash);
        assert!(key.starts_with("chat:linkpreview:"));
        assert_eq!(key.len(), "chat:linkpreview:".len() + 64); // SHA256 hex is 64 chars
    }

    #[test]
    fn test_preview_status_strings() {
        assert_eq!(PreviewStatus::Pending.as_str(), "pending");
        assert_eq!(PreviewStatus::Success.as_str(), "success");
        assert_eq!(PreviewStatus::Failed.as_str(), "failed");
    }
}
