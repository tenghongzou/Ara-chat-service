-- Migration: 009_message_pins.sql
-- Description: Add message pinning support
-- Created: 2026-01-04

-- ============================================================================
-- Message Pins Table
-- ============================================================================
-- Stores pinned messages per conversation. Pins are typically few per
-- conversation, so no partitioning is needed.

CREATE TABLE IF NOT EXISTS message_pins (
    message_id UUID NOT NULL,
    conversation_id UUID NOT NULL,
    pinned_by UUID NOT NULL,
    pinned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (message_id, conversation_id)
);

-- Index for fetching all pins in a conversation (ordered by pin time)
CREATE INDEX idx_pins_conversation ON message_pins(conversation_id, pinned_at DESC);

-- Index for checking if a specific message is pinned
CREATE INDEX idx_pins_message ON message_pins(message_id);

-- ============================================================================
-- Comments
-- ============================================================================

COMMENT ON TABLE message_pins IS 'Stores pinned messages per conversation';
COMMENT ON COLUMN message_pins.message_id IS 'ID of the pinned message';
COMMENT ON COLUMN message_pins.conversation_id IS 'Conversation where message is pinned';
COMMENT ON COLUMN message_pins.pinned_by IS 'User who pinned the message (for audit)';
COMMENT ON COLUMN message_pins.pinned_at IS 'Timestamp when message was pinned';
