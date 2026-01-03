-- Messages table (partitioned by date for 30-day retention)
-- Note: In production, use pg_partman for automatic partition management

CREATE TABLE IF NOT EXISTS messages (
    id UUID NOT NULL,
    conversation_id UUID NOT NULL,
    sender_id UUID NOT NULL,
    tenant_id VARCHAR(64) NOT NULL DEFAULT 'default',
    content TEXT NOT NULL,
    content_type VARCHAR(20) NOT NULL DEFAULT 'text' CHECK (content_type IN ('text', 'image', 'file', 'system')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ,
    reply_to_id UUID,
    mentions UUID[] DEFAULT '{}',
    client_message_id VARCHAR(64),
    partition_key DATE NOT NULL DEFAULT CURRENT_DATE,
    PRIMARY KEY (id, partition_key)
) PARTITION BY RANGE (partition_key);

-- Create initial partition for current month
-- In production, pg_partman would manage these automatically
CREATE TABLE IF NOT EXISTS messages_default PARTITION OF messages DEFAULT;

-- Index for fetching conversation history
CREATE INDEX IF NOT EXISTS idx_messages_conversation
    ON messages(conversation_id, created_at DESC);

-- GIN index for @mentions queries
CREATE INDEX IF NOT EXISTS idx_messages_mentions
    ON messages USING GIN(mentions);

-- Unique index for client-side message deduplication
CREATE UNIQUE INDEX IF NOT EXISTS idx_messages_dedup
    ON messages(sender_id, client_message_id, partition_key)
    WHERE client_message_id IS NOT NULL;

-- Index for finding messages by sender
CREATE INDEX IF NOT EXISTS idx_messages_sender
    ON messages(tenant_id, sender_id, created_at DESC);
