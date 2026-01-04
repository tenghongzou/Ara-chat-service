-- Migration: 006_attachments
-- Description: Create attachments table for file uploads
-- Created: 2026-01-04

-- Attachments table for file uploads
CREATE TABLE IF NOT EXISTS attachments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Relationships
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    message_id UUID,  -- No FK constraint due to messages partitioning
    uploader_id UUID NOT NULL,

    -- File metadata
    file_name VARCHAR(255) NOT NULL,
    file_size BIGINT NOT NULL,
    mime_type VARCHAR(100) NOT NULL,
    content_hash CHAR(64) NOT NULL,  -- SHA256 for deduplication

    -- Storage info
    storage_backend VARCHAR(20) NOT NULL,  -- 's3' or 'local'
    storage_path TEXT NOT NULL,

    -- Thumbnail (for images)
    thumbnail_path TEXT,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT attachments_file_size_check CHECK (file_size > 0 AND file_size <= 52428800),
    CONSTRAINT attachments_storage_backend_check CHECK (storage_backend IN ('s3', 'local'))
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_attachments_conversation ON attachments(conversation_id);
CREATE INDEX IF NOT EXISTS idx_attachments_message ON attachments(message_id);
CREATE INDEX IF NOT EXISTS idx_attachments_uploader ON attachments(uploader_id);
CREATE INDEX IF NOT EXISTS idx_attachments_content_hash ON attachments(content_hash);
CREATE INDEX IF NOT EXISTS idx_attachments_created_at ON attachments(created_at DESC);

-- Comment
COMMENT ON TABLE attachments IS 'File attachments for chat messages';
COMMENT ON COLUMN attachments.content_hash IS 'SHA256 hash for deduplication';
COMMENT ON COLUMN attachments.storage_backend IS 'Storage backend: s3 or local';
