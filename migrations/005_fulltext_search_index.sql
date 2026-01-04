-- Full-text search GIN index for messages
-- This index significantly improves performance for to_tsvector queries

-- GIN index for full-text search on message content
-- Using 'english' configuration for stemming and stop words
-- Note: In production, consider running this with CONCURRENTLY outside of migrations
CREATE INDEX IF NOT EXISTS idx_messages_content_fts
    ON messages USING GIN(to_tsvector('english', content));
