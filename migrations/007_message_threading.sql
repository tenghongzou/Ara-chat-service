-- Migration: 007_message_threading
-- Description: Add indexes for efficient message threading queries
-- Created: 2026-01-04

-- Index for fetching all replies to a specific message
-- Partial index to only include messages that are replies
CREATE INDEX IF NOT EXISTS idx_messages_reply_to
    ON messages(reply_to_id, created_at DESC)
    WHERE reply_to_id IS NOT NULL;

-- Index for counting replies efficiently
-- Includes conversation_id for validation queries
CREATE INDEX IF NOT EXISTS idx_messages_thread_count
    ON messages(conversation_id, reply_to_id)
    WHERE reply_to_id IS NOT NULL AND deleted_at IS NULL;

-- Comment on indexes
COMMENT ON INDEX idx_messages_reply_to IS 'Index for fetching thread replies by reply_to_id';
COMMENT ON INDEX idx_messages_thread_count IS 'Index for counting non-deleted replies in a conversation';
