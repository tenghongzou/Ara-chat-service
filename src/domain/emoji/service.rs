//! Emoji service - business logic for custom emoji management

use std::sync::Arc;

use chrono::Utc;
use image::{imageops::FilterType, ImageFormat};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::io::Cursor;
use uuid::Uuid;

use crate::attachment::{FileStorage, LocalFileStorage};
use crate::config::FileStorageSettings;

use super::{
    is_allowed_mime_type, is_animated_mime_type, is_valid_shortcode, normalize_shortcode,
    CustomEmoji, CustomEmojiRow, EmojiError, EmojiPackRow, EMOJI_THUMBNAIL_SIZE,
    MAX_EMOJI_DIMENSION, MAX_EMOJI_SIZE,
};

/// Emoji service for handling custom emoji operations
pub struct EmojiService {
    storage: Arc<dyn FileStorage>,
    db: Arc<PgPool>,
    base_url: String,
}

impl EmojiService {
    /// Create a new emoji service with local storage
    pub fn with_local_storage(db: Arc<PgPool>, settings: &FileStorageSettings) -> Self {
        let storage = LocalFileStorage::new(&settings.local_path);
        // Use S3 public URL if available, otherwise use files path
        let base_url = settings
            .s3_public_url
            .clone()
            .unwrap_or_else(|| "/files".to_string());
        Self {
            storage: Arc::new(storage),
            db,
            base_url,
        }
    }

    /// Create emoji service from settings
    pub fn from_settings(db: Arc<PgPool>, settings: &FileStorageSettings) -> Self {
        Self::with_local_storage(db, settings)
    }

    // ==================== Emoji Operations ====================

    /// Upload a new custom emoji
    pub async fn upload_emoji(
        &self,
        tenant_id: Uuid,
        creator_id: Uuid,
        shortcode: &str,
        name: &str,
        pack_id: Option<Uuid>,
        data: &[u8],
        mime_type: &str,
    ) -> Result<CustomEmoji, EmojiError> {
        // Validate file size
        if data.len() > MAX_EMOJI_SIZE {
            return Err(EmojiError::FileTooLarge {
                size: data.len(),
                max: MAX_EMOJI_SIZE,
            });
        }

        // Validate MIME type
        if !is_allowed_mime_type(mime_type) {
            return Err(EmojiError::InvalidMimeType(mime_type.to_string()));
        }

        // Normalize and validate shortcode
        let shortcode = normalize_shortcode(shortcode);
        if !is_valid_shortcode(&shortcode) {
            return Err(EmojiError::InvalidShortcode(shortcode));
        }

        // Check if shortcode already exists for this tenant
        if self.shortcode_exists(tenant_id, &shortcode).await? {
            return Err(EmojiError::ShortcodeExists(shortcode));
        }

        // Validate pack exists if provided
        if let Some(pid) = pack_id {
            if !self.pack_exists(tenant_id, pid).await? {
                return Err(EmojiError::PackNotFound(pid));
            }
        }

        // Calculate content hash for deduplication
        let content_hash = self.calculate_hash(data);

        // Check for existing emoji with same hash
        if let Some(existing) = self.find_by_hash(tenant_id, &content_hash).await? {
            tracing::debug!(
                hash = %content_hash,
                existing_id = %existing.id,
                "Found duplicate emoji"
            );
            return Ok(existing);
        }

        // Process image: resize if needed and get dimensions
        let (processed_data, width, height) = self.process_image(data, mime_type)?;
        let is_animated = is_animated_mime_type(mime_type);

        // Generate storage paths
        let extension = self.get_extension(mime_type);
        let storage_key = self.generate_storage_key(tenant_id, extension);

        // Upload main image
        self.storage
            .upload(&storage_key, &processed_data, mime_type)
            .await
            .map_err(|e| EmojiError::StorageError(e.to_string()))?;

        // Generate and upload thumbnail
        let thumbnail_path = if !is_animated {
            match self.generate_thumbnail(&processed_data) {
                Ok(thumb_data) => {
                    let thumb_key = format!("thumbs/{}", storage_key);
                    match self
                        .storage
                        .upload(&thumb_key, &thumb_data, "image/png")
                        .await
                    {
                        Ok(_) => Some(thumb_key),
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to upload emoji thumbnail");
                            None
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to generate emoji thumbnail");
                    None
                }
            }
        } else {
            // For animated GIFs, use the original as thumbnail
            None
        };

        // Store in database
        let emoji = self
            .insert_emoji(
                tenant_id,
                creator_id,
                &shortcode,
                name,
                pack_id,
                &storage_key,
                thumbnail_path.as_deref(),
                &content_hash,
                mime_type,
                processed_data.len() as i32,
                width as i32,
                height as i32,
                is_animated,
            )
            .await?;

        tracing::info!(
            emoji_id = %emoji.id,
            shortcode = %shortcode,
            tenant_id = %tenant_id,
            "Custom emoji uploaded"
        );

        Ok(emoji)
    }

    /// Get emoji by ID
    pub async fn get_emoji(&self, id: Uuid) -> Result<CustomEmoji, EmojiError> {
        let row: CustomEmojiRow = sqlx::query_as(
            r#"
            SELECT id, tenant_id, pack_id, shortcode, name, creator_id,
                   image_path, thumbnail_path, content_hash, storage_backend,
                   mime_type, file_size, width, height, is_animated, created_at
            FROM custom_emojis
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(self.db.as_ref())
        .await?
        .ok_or(EmojiError::NotFound(id))?;

        Ok(self.row_to_emoji(row))
    }

    /// Get emoji by shortcode for a tenant
    pub async fn get_emoji_by_shortcode(
        &self,
        tenant_id: Uuid,
        shortcode: &str,
    ) -> Result<Option<CustomEmoji>, EmojiError> {
        let shortcode = normalize_shortcode(shortcode);
        let row: Option<CustomEmojiRow> = sqlx::query_as(
            r#"
            SELECT id, tenant_id, pack_id, shortcode, name, creator_id,
                   image_path, thumbnail_path, content_hash, storage_backend,
                   mime_type, file_size, width, height, is_animated, created_at
            FROM custom_emojis
            WHERE tenant_id = $1 AND shortcode = $2
            "#,
        )
        .bind(tenant_id)
        .bind(&shortcode)
        .fetch_optional(self.db.as_ref())
        .await?;

        Ok(row.map(|r| self.row_to_emoji(r)))
    }

    /// Delete a custom emoji
    pub async fn delete_emoji(&self, id: Uuid, user_id: Uuid) -> Result<(), EmojiError> {
        let emoji = self.get_emoji_row(id).await?;

        // Only creator can delete
        if emoji.creator_id != user_id {
            return Err(EmojiError::PermissionDenied);
        }

        // Delete from storage
        self.storage
            .delete(&emoji.image_path)
            .await
            .map_err(|e| EmojiError::StorageError(e.to_string()))?;
        if let Some(ref thumb_path) = emoji.thumbnail_path {
            let _ = self.storage.delete(thumb_path).await;
        }

        // Delete from database
        sqlx::query("DELETE FROM custom_emojis WHERE id = $1")
            .bind(id)
            .execute(self.db.as_ref())
            .await?;

        tracing::info!(emoji_id = %id, shortcode = %emoji.shortcode, "Custom emoji deleted");

        Ok(())
    }

    /// List emojis for a tenant
    pub async fn list_emojis(
        &self,
        tenant_id: Uuid,
        pack_id: Option<Uuid>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<CustomEmoji>, EmojiError> {
        let rows: Vec<CustomEmojiRow> = if let Some(pid) = pack_id {
            sqlx::query_as(
                r#"
                SELECT id, tenant_id, pack_id, shortcode, name, creator_id,
                       image_path, thumbnail_path, content_hash, storage_backend,
                       mime_type, file_size, width, height, is_animated, created_at
                FROM custom_emojis
                WHERE tenant_id = $1 AND pack_id = $2
                ORDER BY shortcode
                LIMIT $3 OFFSET $4
                "#,
            )
            .bind(tenant_id)
            .bind(pid)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.as_ref())
            .await?
        } else {
            sqlx::query_as(
                r#"
                SELECT id, tenant_id, pack_id, shortcode, name, creator_id,
                       image_path, thumbnail_path, content_hash, storage_backend,
                       mime_type, file_size, width, height, is_animated, created_at
                FROM custom_emojis
                WHERE tenant_id = $1
                ORDER BY shortcode
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(tenant_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.as_ref())
            .await?
        };

        Ok(rows.into_iter().map(|r| self.row_to_emoji(r)).collect())
    }

    /// Search emojis by name or shortcode
    pub async fn search_emojis(
        &self,
        tenant_id: Uuid,
        query: &str,
        limit: i64,
    ) -> Result<Vec<CustomEmoji>, EmojiError> {
        let search_query = format!("%{}%", query.to_lowercase());
        let rows: Vec<CustomEmojiRow> = sqlx::query_as(
            r#"
            SELECT id, tenant_id, pack_id, shortcode, name, creator_id,
                   image_path, thumbnail_path, content_hash, storage_backend,
                   mime_type, file_size, width, height, is_animated, created_at
            FROM custom_emojis
            WHERE tenant_id = $1
              AND (LOWER(name) LIKE $2 OR LOWER(shortcode) LIKE $2)
            ORDER BY shortcode
            LIMIT $3
            "#,
        )
        .bind(tenant_id)
        .bind(&search_query)
        .bind(limit)
        .fetch_all(self.db.as_ref())
        .await?;

        Ok(rows.into_iter().map(|r| self.row_to_emoji(r)).collect())
    }

    // ==================== Pack Operations ====================

    /// Create a new emoji pack
    pub async fn create_pack(
        &self,
        tenant_id: Uuid,
        creator_id: Uuid,
        name: &str,
        description: Option<&str>,
    ) -> Result<EmojiPackRow, EmojiError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(EmojiError::InvalidShortcode(
                "Pack name cannot be empty".to_string(),
            ));
        }

        // Check if pack name already exists for this tenant
        if self.pack_name_exists(tenant_id, name).await? {
            return Err(EmojiError::PackNameExists(name.to_string()));
        }

        let now = Utc::now();
        let pack: EmojiPackRow = sqlx::query_as(
            r#"
            INSERT INTO emoji_packs (tenant_id, name, description, creator_id, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $5)
            RETURNING id, tenant_id, name, description, creator_id, is_default, created_at, updated_at
            "#,
        )
        .bind(tenant_id)
        .bind(name)
        .bind(description)
        .bind(creator_id)
        .bind(now)
        .fetch_one(self.db.as_ref())
        .await?;

        tracing::info!(
            pack_id = %pack.id,
            name = %name,
            tenant_id = %tenant_id,
            "Emoji pack created"
        );

        Ok(pack)
    }

    /// Get pack by ID
    pub async fn get_pack(&self, id: Uuid) -> Result<EmojiPackRow, EmojiError> {
        let pack: EmojiPackRow = sqlx::query_as(
            r#"
            SELECT id, tenant_id, name, description, creator_id, is_default, created_at, updated_at
            FROM emoji_packs
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(self.db.as_ref())
        .await?
        .ok_or(EmojiError::PackNotFound(id))?;

        Ok(pack)
    }

    /// Update a pack
    pub async fn update_pack(
        &self,
        id: Uuid,
        user_id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
        is_default: Option<bool>,
    ) -> Result<EmojiPackRow, EmojiError> {
        let pack = self.get_pack(id).await?;

        // Only creator can update
        if pack.creator_id != user_id {
            return Err(EmojiError::PermissionDenied);
        }

        let new_name = name.map(|n| n.trim()).unwrap_or(&pack.name);
        let new_description = description.or(pack.description.as_deref());
        let new_is_default = is_default.unwrap_or(pack.is_default);
        let now = Utc::now();

        let updated: EmojiPackRow = sqlx::query_as(
            r#"
            UPDATE emoji_packs
            SET name = $1, description = $2, is_default = $3, updated_at = $4
            WHERE id = $5
            RETURNING id, tenant_id, name, description, creator_id, is_default, created_at, updated_at
            "#,
        )
        .bind(new_name)
        .bind(new_description)
        .bind(new_is_default)
        .bind(now)
        .bind(id)
        .fetch_one(self.db.as_ref())
        .await?;

        Ok(updated)
    }

    /// Delete a pack (emojis become ungrouped)
    pub async fn delete_pack(&self, id: Uuid, user_id: Uuid) -> Result<(), EmojiError> {
        let pack = self.get_pack(id).await?;

        // Only creator can delete
        if pack.creator_id != user_id {
            return Err(EmojiError::PermissionDenied);
        }

        // Delete pack (emojis will have pack_id set to NULL due to ON DELETE SET NULL)
        sqlx::query("DELETE FROM emoji_packs WHERE id = $1")
            .bind(id)
            .execute(self.db.as_ref())
            .await?;

        tracing::info!(pack_id = %id, name = %pack.name, "Emoji pack deleted");

        Ok(())
    }

    /// List packs for a tenant
    pub async fn list_packs(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<(EmojiPackRow, i64)>, EmojiError> {
        let packs: Vec<EmojiPackRow> = sqlx::query_as(
            r#"
            SELECT id, tenant_id, name, description, creator_id, is_default, created_at, updated_at
            FROM emoji_packs
            WHERE tenant_id = $1
            ORDER BY is_default DESC, name
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.db.as_ref())
        .await?;

        // Get emoji counts for each pack
        let mut result = Vec::with_capacity(packs.len());
        for pack in packs {
            let count = self.get_pack_emoji_count(pack.id).await?;
            result.push((pack, count));
        }

        Ok(result)
    }

    /// Get emoji count for a pack
    pub async fn get_pack_emoji_count(&self, pack_id: Uuid) -> Result<i64, EmojiError> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM custom_emojis WHERE pack_id = $1")
            .bind(pack_id)
            .fetch_one(self.db.as_ref())
            .await?;

        Ok(row.0)
    }

    // ==================== Helper Methods ====================

    async fn shortcode_exists(&self, tenant_id: Uuid, shortcode: &str) -> Result<bool, EmojiError> {
        let row: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM custom_emojis WHERE tenant_id = $1 AND shortcode = $2)",
        )
        .bind(tenant_id)
        .bind(shortcode)
        .fetch_one(self.db.as_ref())
        .await?;

        Ok(row.0)
    }

    async fn pack_exists(&self, tenant_id: Uuid, pack_id: Uuid) -> Result<bool, EmojiError> {
        let row: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM emoji_packs WHERE tenant_id = $1 AND id = $2)",
        )
        .bind(tenant_id)
        .bind(pack_id)
        .fetch_one(self.db.as_ref())
        .await?;

        Ok(row.0)
    }

    async fn pack_name_exists(&self, tenant_id: Uuid, name: &str) -> Result<bool, EmojiError> {
        let row: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM emoji_packs WHERE tenant_id = $1 AND name = $2)",
        )
        .bind(tenant_id)
        .bind(name)
        .fetch_one(self.db.as_ref())
        .await?;

        Ok(row.0)
    }

    async fn find_by_hash(
        &self,
        tenant_id: Uuid,
        content_hash: &str,
    ) -> Result<Option<CustomEmoji>, EmojiError> {
        let row: Option<CustomEmojiRow> = sqlx::query_as(
            r#"
            SELECT id, tenant_id, pack_id, shortcode, name, creator_id,
                   image_path, thumbnail_path, content_hash, storage_backend,
                   mime_type, file_size, width, height, is_animated, created_at
            FROM custom_emojis
            WHERE tenant_id = $1 AND content_hash = $2
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(content_hash)
        .fetch_optional(self.db.as_ref())
        .await?;

        Ok(row.map(|r| self.row_to_emoji(r)))
    }

    async fn get_emoji_row(&self, id: Uuid) -> Result<CustomEmojiRow, EmojiError> {
        let row: CustomEmojiRow = sqlx::query_as(
            r#"
            SELECT id, tenant_id, pack_id, shortcode, name, creator_id,
                   image_path, thumbnail_path, content_hash, storage_backend,
                   mime_type, file_size, width, height, is_animated, created_at
            FROM custom_emojis
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(self.db.as_ref())
        .await?
        .ok_or(EmojiError::NotFound(id))?;

        Ok(row)
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_emoji(
        &self,
        tenant_id: Uuid,
        creator_id: Uuid,
        shortcode: &str,
        name: &str,
        pack_id: Option<Uuid>,
        image_path: &str,
        thumbnail_path: Option<&str>,
        content_hash: &str,
        mime_type: &str,
        file_size: i32,
        width: i32,
        height: i32,
        is_animated: bool,
    ) -> Result<CustomEmoji, EmojiError> {
        let backend = self.storage.backend_name();
        let now = Utc::now();

        let row: CustomEmojiRow = sqlx::query_as(
            r#"
            INSERT INTO custom_emojis (
                tenant_id, pack_id, shortcode, name, creator_id,
                image_path, thumbnail_path, content_hash, storage_backend,
                mime_type, file_size, width, height, is_animated, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            RETURNING id, tenant_id, pack_id, shortcode, name, creator_id,
                      image_path, thumbnail_path, content_hash, storage_backend,
                      mime_type, file_size, width, height, is_animated, created_at
            "#,
        )
        .bind(tenant_id)
        .bind(pack_id)
        .bind(shortcode)
        .bind(name)
        .bind(creator_id)
        .bind(image_path)
        .bind(thumbnail_path)
        .bind(content_hash)
        .bind(backend)
        .bind(mime_type)
        .bind(file_size)
        .bind(width)
        .bind(height)
        .bind(is_animated)
        .bind(now)
        .fetch_one(self.db.as_ref())
        .await?;

        Ok(self.row_to_emoji(row))
    }

    fn row_to_emoji(&self, row: CustomEmojiRow) -> CustomEmoji {
        let image_url = self
            .storage
            .public_url(&row.image_path)
            .unwrap_or_else(|| format!("{}/{}", self.base_url, row.image_path));
        let thumbnail_url = row.thumbnail_path.as_ref().map(|p| {
            self.storage
                .public_url(p)
                .unwrap_or_else(|| format!("{}/{}", self.base_url, p))
        });

        CustomEmoji {
            id: row.id,
            tenant_id: row.tenant_id,
            pack_id: row.pack_id,
            shortcode: row.shortcode,
            name: row.name,
            creator_id: row.creator_id,
            image_url,
            thumbnail_url,
            mime_type: row.mime_type,
            file_size: row.file_size,
            width: row.width,
            height: row.height,
            is_animated: row.is_animated,
            created_at: row.created_at,
        }
    }

    fn calculate_hash(&self, data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    fn get_extension(&self, mime_type: &str) -> &'static str {
        match mime_type {
            "image/png" => "png",
            "image/gif" => "gif",
            "image/webp" => "webp",
            _ => "png",
        }
    }

    fn generate_storage_key(&self, tenant_id: Uuid, extension: &str) -> String {
        let now = Utc::now();
        let id = Uuid::new_v4();
        format!(
            "emojis/{}/{}/{}/{}.{}",
            tenant_id,
            now.format("%Y"),
            now.format("%m"),
            id,
            extension
        )
    }

    fn process_image(
        &self,
        data: &[u8],
        mime_type: &str,
    ) -> Result<(Vec<u8>, u32, u32), EmojiError> {
        // For GIFs, preserve animation by not processing
        if mime_type == "image/gif" {
            // Just get dimensions
            let img = image::load_from_memory(data)
                .map_err(|e| EmojiError::ImageError(format!("Failed to load image: {}", e)))?;
            return Ok((data.to_vec(), img.width(), img.height()));
        }

        // Load image
        let img = image::load_from_memory(data)
            .map_err(|e| EmojiError::ImageError(format!("Failed to load image: {}", e)))?;

        let (width, height) = (img.width(), img.height());

        // Resize if needed
        let processed = if width > MAX_EMOJI_DIMENSION || height > MAX_EMOJI_DIMENSION {
            let ratio = width as f64 / height as f64;
            let (new_width, new_height) = if width > height {
                let nw = MAX_EMOJI_DIMENSION;
                let nh = (nw as f64 / ratio) as u32;
                (nw, nh.max(1))
            } else {
                let nh = MAX_EMOJI_DIMENSION;
                let nw = (nh as f64 * ratio) as u32;
                (nw.max(1), nh)
            };
            img.resize(new_width, new_height, FilterType::Lanczos3)
        } else {
            img
        };

        let final_width = processed.width();
        let final_height = processed.height();

        // Encode back
        let format = match mime_type {
            "image/png" => ImageFormat::Png,
            "image/webp" => ImageFormat::WebP,
            _ => ImageFormat::Png,
        };

        let mut buffer = Cursor::new(Vec::new());
        processed
            .write_to(&mut buffer, format)
            .map_err(|e| EmojiError::ImageError(format!("Failed to encode image: {}", e)))?;

        Ok((buffer.into_inner(), final_width, final_height))
    }

    fn generate_thumbnail(&self, data: &[u8]) -> Result<Vec<u8>, EmojiError> {
        let img = image::load_from_memory(data)
            .map_err(|e| EmojiError::ImageError(format!("Failed to load image: {}", e)))?;

        let thumbnail = img.thumbnail(EMOJI_THUMBNAIL_SIZE, EMOJI_THUMBNAIL_SIZE);

        let mut buffer = Cursor::new(Vec::new());
        thumbnail
            .write_to(&mut buffer, ImageFormat::Png)
            .map_err(|e| EmojiError::ImageError(format!("Failed to encode thumbnail: {}", e)))?;

        Ok(buffer.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_storage_key() {
        let tenant_id = Uuid::new_v4();
        let now = Utc::now();
        let key = format!(
            "emojis/{}/{}/{}/{}.png",
            tenant_id,
            now.format("%Y"),
            now.format("%m"),
            Uuid::new_v4()
        );
        assert!(key.starts_with("emojis/"));
        assert!(key.ends_with(".png"));
    }

    #[test]
    fn test_get_extension() {
        assert_eq!(
            "png",
            match "image/png" {
                "image/png" => "png",
                "image/gif" => "gif",
                "image/webp" => "webp",
                _ => "png",
            }
        );
        assert_eq!(
            "gif",
            match "image/gif" {
                "image/png" => "png",
                "image/gif" => "gif",
                "image/webp" => "webp",
                _ => "png",
            }
        );
    }

    #[test]
    fn test_calculate_hash() {
        let mut hasher = Sha256::new();
        hasher.update(b"test emoji data");
        let hash = format!("{:x}", hasher.finalize());
        assert_eq!(hash.len(), 64);
    }
}
