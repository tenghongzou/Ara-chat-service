//! GDPR Service
//!
//! Main orchestrator for GDPR compliance operations including:
//! - Data export (Art. 20 - Data Portability)
//! - Data deletion (Art. 17 - Right to Erasure)
//! - Audit log access (Art. 15 - Right of Access)

use std::path::PathBuf;
use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;

use super::audit::GdprAuditLogger;
use super::deletion::DataDeleter;
use super::error::GdprError;
use super::export::DataExporter;
use super::types::{
    AuditLogEntry, DeletionOptions, DeletionResult, ExportResult, GdprRequestContext,
    GdprRequestStatus, RequesterType,
};

/// Configuration for the GDPR service
#[derive(Debug, Clone)]
pub struct GdprServiceConfig {
    /// Whether GDPR features are enabled
    pub enabled: bool,
    /// Base path for export files
    pub export_path: PathBuf,
    /// Days to retain export files
    pub export_retention_days: u32,
    /// Years to retain audit logs
    pub audit_log_retention_years: u32,
}

impl Default for GdprServiceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            export_path: PathBuf::from("./gdpr-exports"),
            export_retention_days: 7,
            audit_log_retention_years: 7,
        }
    }
}

/// Main GDPR service for compliance operations
pub struct GdprService {
    audit_logger: Arc<GdprAuditLogger>,
    exporter: DataExporter,
    deleter: DataDeleter,
    config: GdprServiceConfig,
    tenant_id: String,
}

impl GdprService {
    /// Create a new GDPR service
    pub fn new(pool: Arc<PgPool>, config: GdprServiceConfig) -> Self {
        let audit_logger = Arc::new(GdprAuditLogger::new(pool.clone()));

        Self {
            exporter: DataExporter::new(
                pool.clone(),
                audit_logger.clone(),
                config.export_path.clone(),
            ),
            deleter: DataDeleter::new(pool, audit_logger.clone()),
            audit_logger,
            config,
            tenant_id: "default".to_string(),
        }
    }

    /// Create a GDPR service with a specific tenant
    pub fn with_tenant(mut self, tenant_id: String) -> Self {
        self.tenant_id = tenant_id.clone();
        self.exporter = self.exporter.with_tenant(tenant_id.clone());
        self.deleter = self.deleter.with_tenant(tenant_id);
        self
    }

    /// Check if GDPR features are enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Request data export for a user (GDPR Art. 20)
    ///
    /// # Arguments
    /// * `user_id` - The user whose data to export
    /// * `requester_id` - Who is requesting the export (usually the same user)
    /// * `requester_type` - Type of requester (user, admin, system)
    /// * `include_attachments` - Whether to include attachment files in export
    /// * `request_ip` - Client IP for audit logging
    /// * `request_user_agent` - Client user agent for audit logging
    pub async fn request_export(
        &self,
        user_id: Uuid,
        requester_id: Option<Uuid>,
        requester_type: RequesterType,
        include_attachments: bool,
        request_ip: Option<String>,
        request_user_agent: Option<String>,
    ) -> Result<ExportResult, GdprError> {
        if !self.config.enabled {
            return Err(GdprError::ServiceUnavailable(
                "GDPR features are disabled".to_string(),
            ));
        }

        let ctx = GdprRequestContext::new(user_id, self.tenant_id.clone())
            .with_requester(requester_id.unwrap_or(user_id), requester_type)
            .with_request_info(request_ip, request_user_agent);

        self.exporter.export_user_data(ctx, include_attachments).await
    }

    /// Request data deletion for a user (GDPR Art. 17)
    ///
    /// # Arguments
    /// * `user_id` - The user whose data to delete
    /// * `requester_id` - Who is requesting the deletion
    /// * `requester_type` - Type of requester
    /// * `options` - Deletion options (anonymize vs hard delete, etc.)
    /// * `request_ip` - Client IP for audit logging
    /// * `request_user_agent` - Client user agent for audit logging
    pub async fn request_deletion(
        &self,
        user_id: Uuid,
        requester_id: Option<Uuid>,
        requester_type: RequesterType,
        options: DeletionOptions,
        request_ip: Option<String>,
        request_user_agent: Option<String>,
    ) -> Result<DeletionResult, GdprError> {
        if !self.config.enabled {
            return Err(GdprError::ServiceUnavailable(
                "GDPR features are disabled".to_string(),
            ));
        }

        let ctx = GdprRequestContext::new(user_id, self.tenant_id.clone())
            .with_requester(requester_id.unwrap_or(user_id), requester_type)
            .with_request_info(request_ip, request_user_agent);

        self.deleter.delete_user_data(ctx, options).await
    }

    /// Get audit log for a user (GDPR Art. 15)
    pub async fn get_audit_log(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<AuditLogEntry>, GdprError> {
        if !self.config.enabled {
            return Err(GdprError::ServiceUnavailable(
                "GDPR features are disabled".to_string(),
            ));
        }

        let limit = limit.min(100).max(1);
        self.audit_logger
            .get_user_audit_logs(&self.tenant_id, user_id, limit)
            .await
    }

    /// Get status of a specific GDPR request
    pub async fn get_request_status(
        &self,
        request_id: Uuid,
    ) -> Result<Option<GdprRequestStatus>, GdprError> {
        self.audit_logger.get_request_status(request_id).await
    }

    /// Get details of a specific GDPR request
    pub async fn get_request_details(
        &self,
        request_id: Uuid,
    ) -> Result<Option<AuditLogEntry>, GdprError> {
        self.audit_logger.get_by_request_id(request_id).await
    }

    /// Check if user has a pending GDPR request
    pub async fn has_pending_request(
        &self,
        user_id: Uuid,
        action_type: &str,
    ) -> Result<bool, GdprError> {
        self.audit_logger
            .has_pending_request(&self.tenant_id, user_id, action_type)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gdpr_service_config_default() {
        let config = GdprServiceConfig::default();
        assert!(config.enabled);
        assert_eq!(config.export_retention_days, 7);
        assert_eq!(config.audit_log_retention_years, 7);
    }

    #[test]
    fn test_gdpr_service_config_custom() {
        let config = GdprServiceConfig {
            enabled: false,
            export_path: PathBuf::from("/custom/path"),
            export_retention_days: 30,
            audit_log_retention_years: 10,
        };

        assert!(!config.enabled);
        assert_eq!(config.export_path, PathBuf::from("/custom/path"));
    }
}
