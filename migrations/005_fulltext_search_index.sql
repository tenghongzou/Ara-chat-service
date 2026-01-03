-- Full-text search GIN index for messages
-- This index significantly improves performance for to_tsvector queries

-- GIN index for full-text search on message content
-- Using 'english' configuration for stemming and stop words
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_messages_content_fts
    ON messages USING GIN(to_tsvector('english', content));

-- Note: CONCURRENTLY allows the index to be built without locking writes
-- This may take time on large tables but won't block operations
