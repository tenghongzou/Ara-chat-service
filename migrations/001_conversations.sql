-- Conversations table
CREATE TABLE IF NOT EXISTS conversations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id VARCHAR(64) NOT NULL DEFAULT 'default',
    type VARCHAR(20) NOT NULL CHECK (type IN ('direct', 'group')),
    name VARCHAR(255),
    avatar_url TEXT,
    created_by UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    participant_count INT NOT NULL DEFAULT 0,
    last_message_id UUID,
    last_message_at TIMESTAMPTZ
);

-- Index for listing user's conversations
CREATE INDEX IF NOT EXISTS idx_conversations_tenant
    ON conversations(tenant_id, updated_at DESC);

-- Conversation participants table
CREATE TABLE IF NOT EXISTS conversation_participants (
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    user_id UUID NOT NULL,
    tenant_id VARCHAR(64) NOT NULL DEFAULT 'default',
    role VARCHAR(20) NOT NULL DEFAULT 'member' CHECK (role IN ('owner', 'admin', 'member')),
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    left_at TIMESTAMPTZ,
    last_read_message_id UUID,
    last_read_at TIMESTAMPTZ,
    PRIMARY KEY (conversation_id, user_id)
);

-- Index for finding user's active conversations
CREATE INDEX IF NOT EXISTS idx_participants_user
    ON conversation_participants(tenant_id, user_id)
    WHERE left_at IS NULL;

-- Direct message lookup table for O(1) private chat lookups
CREATE TABLE IF NOT EXISTS direct_message_lookup (
    user_pair_hash BYTEA PRIMARY KEY,
    conversation_id UUID NOT NULL REFERENCES conversations(id),
    user1_id UUID NOT NULL,
    user2_id UUID NOT NULL,
    tenant_id VARCHAR(64) NOT NULL DEFAULT 'default'
);

-- Index for finding by users
CREATE INDEX IF NOT EXISTS idx_dm_lookup_users
    ON direct_message_lookup(tenant_id, user1_id, user2_id);
