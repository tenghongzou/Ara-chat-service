//! GDPR Compliance Module
//!
//! Provides GDPR compliance features for the Chat Service:
//!
//! - **Data Export (Art. 20)**: Export user data in machine-readable format
//! - **Data Deletion (Art. 17)**: Delete/anonymize user data (Right to Erasure)
//! - **Audit Logging (Art. 30)**: Track GDPR-related operations
//!
//! # Usage
//!
//! ```ignore
//! use ara_chat_service::gdpr::{GdprService, GdprServiceConfig, DeletionOptions};
//!
//! let config = GdprServiceConfig::default();
//! let service = GdprService::new(pool, config);
//!
//! // Export user data
//! let export = service.request_export(user_id, None, RequesterType::User, true, None, None).await?;
//!
//! // Delete user data
//! let result = service.request_deletion(user_id, None, RequesterType::User, DeletionOptions::default(), None, None).await?;
//! ```

mod audit;
mod deletion;
mod error;
mod export;
mod service;
mod types;

// Re-export main types
pub use audit::GdprAuditLogger;
pub use deletion::DataDeleter;
pub use error::GdprError;
pub use export::DataExporter;
pub use service::{GdprService, GdprServiceConfig};
pub use types::{
    AffectedDataSummary, AttachmentExport, AuditLogEntry, ConversationExport, DeletionOptions,
    DeletionResult, ExportMetadata, ExportResult, GdprActionType, GdprRequestContext,
    GdprRequestStatus, MessageExport, ReactionExport, RequesterType, UserDataExport,
    UserProfileExport,
};
