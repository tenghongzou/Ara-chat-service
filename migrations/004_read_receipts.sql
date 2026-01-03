-- Read receipts table
CREATE TABLE IF NOT EXISTS read_receipts (
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    user_id UUID NOT NULL,
    last_read_message_id UUID NOT NULL,
    last_read_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (conversation_id, user_id)
);

-- Index for finding who has read up to a specific message
CREATE INDEX IF NOT EXISTS idx_read_receipts_message
    ON read_receipts(conversation_id, last_read_message_id);

-- Index for user's read status across conversations
CREATE INDEX IF NOT EXISTS idx_read_receipts_user
    ON read_receipts(user_id, last_read_at DESC);
