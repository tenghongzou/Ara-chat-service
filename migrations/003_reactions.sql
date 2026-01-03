-- Message reactions table (partitioned like messages)
CREATE TABLE IF NOT EXISTS message_reactions (
    message_id UUID NOT NULL,
    message_partition_key DATE NOT NULL,
    user_id UUID NOT NULL,
    emoji VARCHAR(32) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (message_id, message_partition_key, user_id, emoji)
) PARTITION BY RANGE (message_partition_key);

-- Create default partition
CREATE TABLE IF NOT EXISTS message_reactions_default PARTITION OF message_reactions DEFAULT;

-- Index for fetching reactions for a message
CREATE INDEX IF NOT EXISTS idx_reactions_message
    ON message_reactions(message_id, message_partition_key);

-- Index for user's reactions
CREATE INDEX IF NOT EXISTS idx_reactions_user
    ON message_reactions(user_id, created_at DESC);
