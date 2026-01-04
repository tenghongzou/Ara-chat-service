-- GDPR Audit Logs table for compliance tracking
-- Stores records of data exports, deletions, and other GDPR-related actions

CREATE TABLE IF NOT EXISTS gdpr_audit_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id VARCHAR(64) NOT NULL DEFAULT 'default',

    -- Action details
    action_type VARCHAR(50) NOT NULL CHECK (action_type IN (
        'DATA_EXPORT_REQUESTED',
        'DATA_EXPORT_COMPLETED',
        'DATA_EXPORT_FAILED',
        'DATA_DELETION_REQUESTED',
        'DATA_DELETION_COMPLETED',
        'DATA_DELETION_FAILED',
        'DATA_ACCESS_REQUESTED'
    )),

    -- Subject user (whose data is affected)
    subject_user_id UUID NOT NULL,

    -- Requester (who initiated the action)
    requester_user_id UUID,
    requester_type VARCHAR(20) NOT NULL DEFAULT 'user' CHECK (requester_type IN ('user', 'admin', 'system')),

    -- Request tracking
    request_id UUID NOT NULL,
    request_ip INET,
    request_user_agent TEXT,

    -- Status tracking
    status VARCHAR(20) NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'processing', 'completed', 'failed')),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,

    -- Affected data summary (JSON for flexibility)
    affected_data JSONB DEFAULT '{}',

    -- Error details if failed
    error_message TEXT,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Retention: GDPR audit logs should be retained for compliance period (7 years)
    expires_at TIMESTAMPTZ DEFAULT (NOW() + INTERVAL '7 years')
);

-- Index for querying by tenant and time (most common query pattern)
CREATE INDEX IF NOT EXISTS idx_gdpr_audit_tenant_created
    ON gdpr_audit_logs(tenant_id, created_at DESC);

-- Index for querying by subject user (for user audit log requests)
CREATE INDEX IF NOT EXISTS idx_gdpr_audit_subject_user
    ON gdpr_audit_logs(tenant_id, subject_user_id, created_at DESC);

-- Index for looking up specific requests
CREATE INDEX IF NOT EXISTS idx_gdpr_audit_request_id
    ON gdpr_audit_logs(request_id);

-- Index for finding pending/processing requests
CREATE INDEX IF NOT EXISTS idx_gdpr_audit_status_pending
    ON gdpr_audit_logs(status, created_at)
    WHERE status IN ('pending', 'processing');

-- Index for action type queries
CREATE INDEX IF NOT EXISTS idx_gdpr_audit_action_type
    ON gdpr_audit_logs(tenant_id, action_type, created_at DESC);

-- Comments
COMMENT ON TABLE gdpr_audit_logs IS 'GDPR compliance audit trail for data processing activities';
COMMENT ON COLUMN gdpr_audit_logs.subject_user_id IS 'The user whose data is being processed';
COMMENT ON COLUMN gdpr_audit_logs.requester_user_id IS 'The user/admin who initiated the request';
COMMENT ON COLUMN gdpr_audit_logs.request_id IS 'Groups multiple log entries for a single GDPR request';
COMMENT ON COLUMN gdpr_audit_logs.affected_data IS 'JSON summary of what data was affected (counts, sizes)';
COMMENT ON COLUMN gdpr_audit_logs.expires_at IS 'Retention period for audit logs (7 years default per GDPR)';
