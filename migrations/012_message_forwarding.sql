-- Migration: 012_message_forwarding.sql
-- Description: Add message forwarding support
-- Created: 2026-01-04

-- ============================================================================
-- Add forwarding columns to messages table
-- ============================================================================

-- Add forwarding metadata columns
ALTER TABLE messages
ADD COLUMN IF NOT EXISTS forwarded_from_message_id UUID,
ADD COLUMN IF NOT EXISTS forwarded_from_sender_id UUID,
ADD COLUMN IF NOT EXISTS forwarded_from_conversation_id UUID;

-- Index for finding all forwards of a specific message (for analytics/audit)
CREATE INDEX IF NOT EXISTS idx_messages_forwarded_from
    ON messages(forwarded_from_message_id)
    WHERE forwarded_from_message_id IS NOT NULL;

-- ============================================================================
-- Comments
-- ============================================================================

COMMENT ON COLUMN messages.forwarded_from_message_id IS 'ID of the original message this was forwarded from';
COMMENT ON COLUMN messages.forwarded_from_sender_id IS 'Original sender ID (denormalized for display without JOIN)';
COMMENT ON COLUMN messages.forwarded_from_conversation_id IS 'Original conversation ID (for audit trail)';
