//! Email backend implementations

mod sendgrid;
mod smtp;

pub use sendgrid::SendGridBackend;
pub use smtp::SmtpBackend;

use async_trait::async_trait;

use super::error::EmailError;
use super::types::EmailMessage;
use crate::config::{EmailBackend, EmailSettings};

/// Email sending backend trait
#[async_trait]
pub trait EmailSender: Send + Sync {
    /// Send an email
    async fn send(&self, email: &EmailMessage) -> Result<(), EmailError>;

    /// Check if the backend is healthy/configured
    fn is_available(&self) -> bool;

    /// Get backend name for logging
    fn name(&self) -> &'static str;
}

/// Create email backend from settings
pub fn create_backend(settings: &EmailSettings) -> Result<Box<dyn EmailSender>, EmailError> {
    match settings.backend {
        EmailBackend::Smtp => {
            let host = settings
                .smtp_host
                .as_ref()
                .ok_or_else(|| EmailError::Configuration("SMTP host required".into()))?;
            let from = settings
                .from_address
                .as_ref()
                .ok_or_else(|| EmailError::Configuration("from_address required".into()))?;

            Ok(Box::new(SmtpBackend::new(
                host.clone(),
                settings.smtp_port.unwrap_or(587),
                settings.smtp_username.clone(),
                settings.smtp_password.clone(),
                settings.smtp_tls,
                from.clone(),
                settings.from_name.clone(),
            )?))
        }
        EmailBackend::SendGrid => {
            let api_key = settings
                .sendgrid_api_key
                .as_ref()
                .ok_or_else(|| EmailError::Configuration("SendGrid API key required".into()))?;
            let from = settings
                .from_address
                .as_ref()
                .ok_or_else(|| EmailError::Configuration("from_address required".into()))?;

            Ok(Box::new(SendGridBackend::new(
                api_key.clone(),
                from.clone(),
                settings.from_name.clone(),
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_backend_smtp_missing_host() {
        let settings = EmailSettings {
            enabled: true,
            backend: EmailBackend::Smtp,
            smtp_host: None,
            from_address: Some("test@example.com".into()),
            ..Default::default()
        };

        let result = create_backend(&settings);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SMTP host"));
    }

    #[test]
    fn test_create_backend_sendgrid_missing_key() {
        let settings = EmailSettings {
            enabled: true,
            backend: EmailBackend::SendGrid,
            sendgrid_api_key: None,
            from_address: Some("test@example.com".into()),
            ..Default::default()
        };

        let result = create_backend(&settings);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SendGrid API key"));
    }
}
