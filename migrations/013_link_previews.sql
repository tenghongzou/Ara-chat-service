-- Migration: 013_link_previews.sql
-- Description: Link preview metadata storage for Open Graph data
-- Feature: Link Preview (v1.6.0)

-- Link preview metadata storage
CREATE TABLE IF NOT EXISTS link_previews (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    message_id UUID NOT NULL,
    url TEXT NOT NULL,
    url_hash VARCHAR(64) NOT NULL,  -- SHA256 for dedup/cache lookup
    title VARCHAR(512),
    description TEXT,
    image_url TEXT,
    site_name VARCHAR(255),
    favicon_url TEXT,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',  -- pending, success, failed
    error TEXT,
    fetched_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT fk_link_previews_message FOREIGN KEY (message_id)
        REFERENCES messages(id) ON DELETE CASCADE
);

-- Index for message lookup (get all previews for a message)
CREATE INDEX IF NOT EXISTS idx_link_previews_message ON link_previews(message_id);

-- Index for cache lookup by URL hash (check if URL already fetched)
CREATE INDEX IF NOT EXISTS idx_link_previews_url_hash ON link_previews(url_hash);

-- Index for finding pending previews (background worker)
CREATE INDEX IF NOT EXISTS idx_link_previews_pending
    ON link_previews(status, created_at)
    WHERE status = 'pending';

-- Prevent duplicate URLs per message
CREATE UNIQUE INDEX IF NOT EXISTS idx_link_previews_message_url
    ON link_previews(message_id, url_hash);

-- Add comment for documentation
COMMENT ON TABLE link_previews IS 'Stores Open Graph metadata extracted from URLs in messages';
COMMENT ON COLUMN link_previews.url_hash IS 'SHA256 hash of URL for deduplication and cache lookup';
COMMENT ON COLUMN link_previews.status IS 'pending: awaiting fetch, success: fetched OK, failed: fetch error';
