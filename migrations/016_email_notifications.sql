-- Migration: 016_email_notifications.sql
-- Description: Email notification queue and user preferences
-- Feature: Email Notifications for Offline Users (v1.9.0)

-- User email preferences
CREATE TABLE IF NOT EXISTS email_preferences (
    user_id UUID NOT NULL,
    tenant_id VARCHAR(64) NOT NULL DEFAULT 'default',
    email_address VARCHAR(255),
    email_enabled BOOLEAN NOT NULL DEFAULT true,
    notify_messages BOOLEAN NOT NULL DEFAULT true,
    notify_mentions BOOLEAN NOT NULL DEFAULT true,
    digest_mode VARCHAR(20) NOT NULL DEFAULT 'immediate',
    quiet_hours_start SMALLINT,  -- Hour in UTC (0-23)
    quiet_hours_end SMALLINT,    -- Hour in UTC (0-23)
    last_email_sent_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, tenant_id),
    CONSTRAINT email_preferences_quiet_hours_valid CHECK (
        (quiet_hours_start IS NULL AND quiet_hours_end IS NULL) OR
        (quiet_hours_start >= 0 AND quiet_hours_start <= 23 AND
         quiet_hours_end >= 0 AND quiet_hours_end <= 23)
    ),
    CONSTRAINT email_preferences_digest_mode_valid CHECK (
        digest_mode IN ('immediate', 'hourly', 'daily')
    )
);

-- Indexes for email_preferences
CREATE INDEX IF NOT EXISTS idx_email_prefs_tenant_user
    ON email_preferences(tenant_id, user_id);

-- Email queue for delayed/batched sending
CREATE TABLE IF NOT EXISTS email_queue (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,
    tenant_id VARCHAR(64) NOT NULL DEFAULT 'default',
    email_type VARCHAR(50) NOT NULL,  -- message, mention, digest
    conversation_id UUID NOT NULL,
    message_ids UUID[] NOT NULL,      -- Array for batching
    sender_ids UUID[] NOT NULL,       -- Senders of the messages
    content_previews TEXT[],          -- Preview snippets
    priority VARCHAR(20) NOT NULL DEFAULT 'normal',
    status VARCHAR(20) NOT NULL DEFAULT 'pending', -- pending, processing, sent, failed, cancelled
    scheduled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    send_after TIMESTAMPTZ NOT NULL,  -- Delay before sending
    sent_at TIMESTAMPTZ,
    error TEXT,
    retry_count SMALLINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT email_queue_type_valid CHECK (
        email_type IN ('message', 'mention', 'digest')
    ),
    CONSTRAINT email_queue_priority_valid CHECK (
        priority IN ('low', 'normal', 'high')
    ),
    CONSTRAINT email_queue_status_valid CHECK (
        status IN ('pending', 'processing', 'sent', 'failed', 'cancelled')
    )
);

-- Indexes for email_queue
CREATE INDEX IF NOT EXISTS idx_email_queue_pending
    ON email_queue(status, send_after)
    WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_email_queue_user
    ON email_queue(tenant_id, user_id, status);

CREATE INDEX IF NOT EXISTS idx_email_queue_conversation
    ON email_queue(user_id, conversation_id, status)
    WHERE status = 'pending';

-- Rate limiting tracking
CREATE TABLE IF NOT EXISTS email_rate_limits (
    user_id UUID NOT NULL,
    tenant_id VARCHAR(64) NOT NULL DEFAULT 'default',
    hour_bucket TIMESTAMPTZ NOT NULL,  -- Truncated to hour
    email_count SMALLINT NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, tenant_id, hour_bucket)
);

CREATE INDEX IF NOT EXISTS idx_email_rate_hour
    ON email_rate_limits(tenant_id, user_id, hour_bucket);

-- Comments
COMMENT ON TABLE email_preferences IS 'User preferences for email notifications';
COMMENT ON TABLE email_queue IS 'Queue for pending email notifications with batching support';
COMMENT ON TABLE email_rate_limits IS 'Hourly rate limiting for email sending';

COMMENT ON COLUMN email_preferences.digest_mode IS 'immediate: send immediately, hourly: batch per hour, daily: daily digest';
COMMENT ON COLUMN email_preferences.quiet_hours_start IS 'Start hour (UTC) for quiet period when no emails are sent';
COMMENT ON COLUMN email_preferences.quiet_hours_end IS 'End hour (UTC) for quiet period';
COMMENT ON COLUMN email_queue.send_after IS 'Delay before sending to allow batching and handle quick reconnects';
COMMENT ON COLUMN email_queue.message_ids IS 'Array of message IDs for batch emails';
COMMENT ON COLUMN email_rate_limits.hour_bucket IS 'Timestamp truncated to hour for rate limit tracking';
