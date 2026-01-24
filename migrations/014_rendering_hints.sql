-- Add rendering hints column to messages table
-- Stores markdown formatting hints for client-side rendering

ALTER TABLE messages
ADD COLUMN IF NOT EXISTS rendering_hints JSONB;

-- Comment explaining the column purpose
COMMENT ON COLUMN messages.rendering_hints IS 'Markdown rendering hints for client formatting (position-based spans)';

-- Optional: Index for analytics queries on messages with markdown formatting
-- This is a partial index that only includes rows with rendering hints
-- Note: Removed CONCURRENTLY as SQLx migrations run in transactions
CREATE INDEX IF NOT EXISTS idx_messages_has_rendering
    ON messages ((rendering_hints IS NOT NULL))
    WHERE rendering_hints IS NOT NULL;
