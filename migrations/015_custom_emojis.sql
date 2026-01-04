-- Migration: 015_custom_emojis
-- Description: Create tables for custom emoji support
-- Created: 2026-01-04

-- Custom emoji packs (optional grouping)
CREATE TABLE IF NOT EXISTS emoji_packs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Tenant isolation
    tenant_id UUID NOT NULL,

    -- Pack metadata
    name VARCHAR(100) NOT NULL,
    description TEXT,
    creator_id UUID NOT NULL,
    is_default BOOLEAN NOT NULL DEFAULT FALSE,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT emoji_packs_name_not_empty CHECK (LENGTH(TRIM(name)) > 0),
    UNIQUE(tenant_id, name)
);

-- Indexes for emoji packs
CREATE INDEX IF NOT EXISTS idx_emoji_packs_tenant ON emoji_packs(tenant_id);
CREATE INDEX IF NOT EXISTS idx_emoji_packs_creator ON emoji_packs(creator_id);

-- Comments for emoji packs
COMMENT ON TABLE emoji_packs IS 'Custom emoji pack groupings';
COMMENT ON COLUMN emoji_packs.tenant_id IS 'Tenant for multi-tenant isolation';
COMMENT ON COLUMN emoji_packs.is_default IS 'Whether this pack is shown by default';

-- Custom emojis
CREATE TABLE IF NOT EXISTS custom_emojis (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Tenant isolation
    tenant_id UUID NOT NULL,

    -- Optional pack reference
    pack_id UUID REFERENCES emoji_packs(id) ON DELETE SET NULL,

    -- Emoji identification
    shortcode VARCHAR(50) NOT NULL,           -- :emoji_name:
    name VARCHAR(100) NOT NULL,               -- Display name

    -- Ownership
    creator_id UUID NOT NULL,

    -- Storage info
    image_path TEXT NOT NULL,                 -- Storage path
    thumbnail_path TEXT,                      -- 64x64 thumbnail
    content_hash CHAR(64) NOT NULL,           -- SHA256 for deduplication
    storage_backend VARCHAR(20) NOT NULL,     -- 's3' or 'local'

    -- File metadata
    mime_type VARCHAR(50) NOT NULL,
    file_size INTEGER NOT NULL,
    width INTEGER,
    height INTEGER,
    is_animated BOOLEAN NOT NULL DEFAULT FALSE,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT custom_emojis_shortcode_format CHECK (
        shortcode ~ '^[a-z0-9_]{2,50}$'
    ),
    CONSTRAINT custom_emojis_name_not_empty CHECK (LENGTH(TRIM(name)) > 0),
    CONSTRAINT custom_emojis_file_size_check CHECK (file_size > 0 AND file_size <= 262144),
    CONSTRAINT custom_emojis_storage_backend_check CHECK (storage_backend IN ('s3', 'local')),
    CONSTRAINT custom_emojis_mime_type_check CHECK (
        mime_type IN ('image/png', 'image/gif', 'image/webp')
    ),
    UNIQUE(tenant_id, shortcode)
);

-- Indexes for custom emojis
CREATE INDEX IF NOT EXISTS idx_custom_emojis_tenant ON custom_emojis(tenant_id);
CREATE INDEX IF NOT EXISTS idx_custom_emojis_pack ON custom_emojis(pack_id);
CREATE INDEX IF NOT EXISTS idx_custom_emojis_shortcode ON custom_emojis(tenant_id, shortcode);
CREATE INDEX IF NOT EXISTS idx_custom_emojis_hash ON custom_emojis(content_hash);
CREATE INDEX IF NOT EXISTS idx_custom_emojis_creator ON custom_emojis(creator_id);
CREATE INDEX IF NOT EXISTS idx_custom_emojis_created ON custom_emojis(created_at DESC);

-- Full-text search on emoji name and shortcode
CREATE INDEX IF NOT EXISTS idx_custom_emojis_search ON custom_emojis
    USING gin(to_tsvector('simple', name || ' ' || shortcode));

-- Comments for custom emojis
COMMENT ON TABLE custom_emojis IS 'User-uploaded custom emojis';
COMMENT ON COLUMN custom_emojis.shortcode IS 'Shortcode format: alphanumeric and underscore only, 2-50 chars';
COMMENT ON COLUMN custom_emojis.content_hash IS 'SHA256 hash for deduplication';
COMMENT ON COLUMN custom_emojis.is_animated IS 'Whether the emoji is animated (GIF)';
