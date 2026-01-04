-- User Blocking
-- Migration: 011_user_blocking.sql
-- Description: Add user blocking support for chat service
-- Version: 1.4.0

-- User blocking table (user-to-user relationship, not conversation-specific)
CREATE TABLE IF NOT EXISTS user_blocks (
    blocker_id UUID NOT NULL,
    blocked_user_id UUID NOT NULL,
    tenant_id VARCHAR(64) NOT NULL DEFAULT 'default',
    blocked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reason VARCHAR(255),
    PRIMARY KEY (blocker_id, blocked_user_id, tenant_id)
);

-- Index for checking if user A has blocked user B
CREATE INDEX IF NOT EXISTS idx_blocks_blocker
    ON user_blocks(tenant_id, blocker_id);

-- Index for checking if user B is blocked by anyone (for reverse lookup)
CREATE INDEX IF NOT EXISTS idx_blocks_blocked
    ON user_blocks(tenant_id, blocked_user_id);

-- Index for getting all blocks involving a user (either direction)
CREATE INDEX IF NOT EXISTS idx_blocks_both_users
    ON user_blocks(tenant_id, blocker_id, blocked_user_id);

-- Comments
COMMENT ON TABLE user_blocks IS 'User blocking relationships for chat';
COMMENT ON COLUMN user_blocks.blocker_id IS 'User who initiated the block';
COMMENT ON COLUMN user_blocks.blocked_user_id IS 'User who was blocked';
COMMENT ON COLUMN user_blocks.blocked_at IS 'When the block was created';
COMMENT ON COLUMN user_blocks.reason IS 'Optional reason for the block';
COMMENT ON COLUMN user_blocks.tenant_id IS 'Tenant ID for multi-tenant support';
