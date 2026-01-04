-- Migration: 010_conversation_muting.sql
-- Description: Add conversation muting support
-- Created: 2026-01-04

-- ============================================================================
-- Add Mute Columns to Conversation Participants
-- ============================================================================
-- Muting is a per-user, per-conversation setting stored directly in the
-- participants table (same pattern as last_read_at).

ALTER TABLE conversation_participants
ADD COLUMN IF NOT EXISTS is_muted BOOLEAN DEFAULT FALSE NOT NULL,
ADD COLUMN IF NOT EXISTS muted_at TIMESTAMPTZ;

-- ============================================================================
-- Indexes
-- ============================================================================

-- Index for querying muted conversations by user (partial index for efficiency)
CREATE INDEX IF NOT EXISTS idx_participants_muted
    ON conversation_participants(tenant_id, user_id)
    WHERE left_at IS NULL AND is_muted = TRUE;

-- Index for getting muted user IDs in a conversation (for notification filtering)
CREATE INDEX IF NOT EXISTS idx_participants_conversation_muted
    ON conversation_participants(conversation_id)
    WHERE left_at IS NULL AND is_muted = TRUE;

-- ============================================================================
-- Comments
-- ============================================================================

COMMENT ON COLUMN conversation_participants.is_muted IS 'Whether user has muted this conversation (no push notifications)';
COMMENT ON COLUMN conversation_participants.muted_at IS 'Timestamp when the conversation was muted';
