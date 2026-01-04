//! Email notification service

use std::sync::Arc;

use chrono::{Timelike, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::backends::{create_backend, EmailSender};
use super::error::EmailError;
use super::queue::EmailQueue;
use super::templates::EmailTemplates;
use super::types::{
    DigestMode, EmailPreferences, EmailPreferencesRow, EmailPriority, EmailType, MessagePreview,
    NotificationContext, QueuedEmail, UpdateEmailPreferencesRequest,
};
use crate::config::EmailSettings;

/// Configuration for the email service
#[derive(Debug, Clone)]
pub struct EmailServiceConfig {
    pub enabled: bool,
    pub delay_seconds: u32,
    pub max_batch_size: usize,
    pub batch_window_seconds: u32,
    pub max_emails_per_hour: u32,
}

/// Main email notification service
pub struct EmailService {
    pool: Arc<PgPool>,
    queue: EmailQueue,
    backend: Box<dyn EmailSender>,
    templates: EmailTemplates,
    config: EmailServiceConfig,
    tenant_id: String,
}

impl EmailService {
    /// Create a new email service from settings
    pub fn new(pool: Arc<PgPool>, settings: &EmailSettings) -> Result<Self, EmailError> {
        if !settings.enabled {
            return Err(EmailError::ServiceUnavailable(
                "Email service disabled".into(),
            ));
        }

        let backend = create_backend(settings)?;
        let templates = EmailTemplates::new(&settings.templates);

        let config = EmailServiceConfig {
            enabled: settings.enabled,
            delay_seconds: settings.delay_seconds,
            max_batch_size: settings.max_batch_size,
            batch_window_seconds: settings.batch_window_seconds,
            max_emails_per_hour: settings.max_emails_per_hour,
        };

        let queue = EmailQueue::new(
            pool.clone(),
            settings.delay_seconds,
            settings.batch_window_seconds,
            settings.max_batch_size,
            settings.max_emails_per_hour,
        );

        Ok(Self {
            pool,
            queue,
            backend,
            templates,
            config,
            tenant_id: "default".to_string(),
        })
    }

    /// Set tenant ID for multi-tenant isolation
    pub fn with_tenant(mut self, tenant_id: String) -> Self {
        self.tenant_id = tenant_id.clone();
        self.queue = EmailQueue::new(
            self.pool.clone(),
            self.config.delay_seconds,
            self.config.batch_window_seconds,
            self.config.max_batch_size,
            self.config.max_emails_per_hour,
        )
        .with_tenant(tenant_id);
        self
    }

    /// Check if the service is enabled and available
    pub fn is_enabled(&self) -> bool {
        self.config.enabled && self.backend.is_available()
    }

    /// Get backend name for logging
    pub fn backend_name(&self) -> &'static str {
        self.backend.name()
    }

    /// Queue email notification for offline user
    pub async fn queue_notification(
        &self,
        user_id: Uuid,
        email_type: EmailType,
        conversation_id: Uuid,
        message_id: Uuid,
        sender_id: Uuid,
        content_preview: Option<String>,
    ) -> Result<Uuid, EmailError> {
        // Check user preferences
        let prefs = self.get_preferences(user_id).await?;

        if !prefs.email_enabled {
            return Err(EmailError::OptedOut);
        }

        if prefs.email_address.is_none() {
            return Err(EmailError::NoEmailAddress(user_id));
        }

        // Check specific notification type
        match email_type {
            EmailType::Message if !prefs.notify_messages => return Err(EmailError::OptedOut),
            EmailType::Mention if !prefs.notify_mentions => return Err(EmailError::OptedOut),
            _ => {}
        }

        // Check quiet hours
        if self.is_quiet_hours(&prefs) {
            return Err(EmailError::QuietHours);
        }

        // Determine priority
        let priority = match email_type {
            EmailType::Mention => EmailPriority::High,
            _ => EmailPriority::Normal,
        };

        // Add to queue
        self.queue
            .enqueue(
                user_id,
                email_type,
                conversation_id,
                message_id,
                sender_id,
                content_preview,
                priority,
            )
            .await
    }

    /// Cancel pending emails (called when user reconnects)
    pub async fn cancel_pending_for_user(&self, user_id: Uuid) -> Result<usize, EmailError> {
        self.queue.cancel_pending(user_id).await
    }

    /// Process pending emails (called by background task)
    pub async fn process_pending(&self, limit: i32) -> Result<usize, EmailError> {
        let emails = self.queue.get_ready_emails(limit).await?;
        let mut sent_count = 0;

        for queued in emails {
            // Mark as processing
            if let Err(e) = self.queue.mark_processing(queued.id).await {
                tracing::warn!(email_id = %queued.id, error = %e, "Failed to mark email as processing");
                continue;
            }

            match self.send_queued_email(&queued).await {
                Ok(_) => {
                    self.queue.mark_sent(queued.id).await?;
                    self.queue.increment_rate_limit(queued.user_id).await?;
                    sent_count += 1;
                    tracing::info!(
                        email_id = %queued.id,
                        user_id = %queued.user_id,
                        email_type = ?queued.email_type,
                        message_count = queued.message_ids.len(),
                        "Email sent successfully"
                    );
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    if e.is_warning() {
                        tracing::warn!(
                            email_id = %queued.id,
                            user_id = %queued.user_id,
                            error = %error_msg,
                            "Failed to send email"
                        );
                    } else {
                        tracing::debug!(
                            email_id = %queued.id,
                            user_id = %queued.user_id,
                            error = %error_msg,
                            "Email not sent"
                        );
                    }
                    self.queue.mark_failed(queued.id, &error_msg).await?;
                }
            }
        }

        Ok(sent_count)
    }

    /// Send a single queued email
    async fn send_queued_email(&self, queued: &QueuedEmail) -> Result<(), EmailError> {
        // Check rate limit
        if !self.queue.check_rate_limit(queued.user_id).await? {
            return Err(EmailError::RateLimited {
                max: self.config.max_emails_per_hour,
            });
        }

        // Get user preferences for email address
        let prefs = self.get_preferences(queued.user_id).await?;
        let to_address = prefs
            .email_address
            .ok_or(EmailError::NoEmailAddress(queued.user_id))?;

        // Build email context
        let context = NotificationContext {
            user_name: None, // TODO: Fetch from user service
            conversation_name: None, // TODO: Fetch from conversation service
            messages: queued
                .content_previews
                .iter()
                .enumerate()
                .map(|(i, preview)| MessagePreview {
                    sender_name: format!(
                        "User {}",
                        queued
                            .sender_ids
                            .get(i)
                            .map(|id| &id.to_string()[..8])
                            .unwrap_or("Unknown")
                    ),
                    content: preview.clone(),
                    timestamp: queued.scheduled_at.format("%H:%M").to_string(),
                    is_mention: queued.email_type == EmailType::Mention,
                })
                .collect(),
            unread_count: queued.message_ids.len(),
            app_url: self.templates.app_url.clone(),
            conversation_url: format!(
                "{}/chat/{}",
                self.templates.app_url, queued.conversation_id
            ),
            unsubscribe_url: self.templates.unsubscribe_url.clone(),
        };

        // Render and send email
        let email = self.templates.render(
            queued.email_type,
            &context,
            &to_address,
            context.user_name.clone(),
        )?;

        self.backend.send(&email).await
    }

    /// Get user email preferences
    pub async fn get_preferences(&self, user_id: Uuid) -> Result<EmailPreferences, EmailError> {
        let row: Option<EmailPreferencesRow> = sqlx::query_as(
            r#"
            SELECT user_id, email_address, email_enabled, notify_messages,
                   notify_mentions, digest_mode, quiet_hours_start, quiet_hours_end,
                   last_email_sent_at
            FROM email_preferences
            WHERE user_id = $1 AND tenant_id = $2
            "#,
        )
        .bind(user_id)
        .bind(&self.tenant_id)
        .fetch_optional(self.pool.as_ref())
        .await?;

        Ok(row.map(|r| r.into()).unwrap_or_else(|| EmailPreferences {
            user_id,
            email_address: None,
            email_enabled: true,
            notify_messages: true,
            notify_mentions: true,
            digest_mode: DigestMode::Immediate,
            quiet_hours_start: None,
            quiet_hours_end: None,
            last_email_sent_at: None,
        }))
    }

    /// Update user email preferences
    pub async fn update_preferences(
        &self,
        user_id: Uuid,
        request: &UpdateEmailPreferencesRequest,
    ) -> Result<EmailPreferences, EmailError> {
        // Get existing preferences or defaults
        let current = self.get_preferences(user_id).await?;

        let email_address = request
            .email_address
            .clone()
            .or(current.email_address);
        let email_enabled = request.email_enabled.unwrap_or(current.email_enabled);
        let notify_messages = request.notify_messages.unwrap_or(current.notify_messages);
        let notify_mentions = request.notify_mentions.unwrap_or(current.notify_mentions);
        let digest_mode = request.digest_mode.unwrap_or(current.digest_mode);
        let quiet_hours_start = request.quiet_hours_start.or(current.quiet_hours_start);
        let quiet_hours_end = request.quiet_hours_end.or(current.quiet_hours_end);

        sqlx::query(
            r#"
            INSERT INTO email_preferences
            (user_id, tenant_id, email_address, email_enabled, notify_messages,
             notify_mentions, digest_mode, quiet_hours_start, quiet_hours_end, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
            ON CONFLICT (user_id, tenant_id) DO UPDATE SET
                email_address = EXCLUDED.email_address,
                email_enabled = EXCLUDED.email_enabled,
                notify_messages = EXCLUDED.notify_messages,
                notify_mentions = EXCLUDED.notify_mentions,
                digest_mode = EXCLUDED.digest_mode,
                quiet_hours_start = EXCLUDED.quiet_hours_start,
                quiet_hours_end = EXCLUDED.quiet_hours_end,
                updated_at = NOW()
            "#,
        )
        .bind(user_id)
        .bind(&self.tenant_id)
        .bind(&email_address)
        .bind(email_enabled)
        .bind(notify_messages)
        .bind(notify_mentions)
        .bind(digest_mode.as_str())
        .bind(quiet_hours_start)
        .bind(quiet_hours_end)
        .execute(self.pool.as_ref())
        .await?;

        Ok(EmailPreferences {
            user_id,
            email_address,
            email_enabled,
            notify_messages,
            notify_mentions,
            digest_mode,
            quiet_hours_start,
            quiet_hours_end,
            last_email_sent_at: current.last_email_sent_at,
        })
    }

    /// Send a test email to verify configuration
    pub async fn send_test_email(&self, to_address: &str) -> Result<(), EmailError> {
        let context = NotificationContext {
            user_name: Some("Test User".into()),
            conversation_name: Some("Test Conversation".into()),
            messages: vec![MessagePreview {
                sender_name: "System".into(),
                content: "This is a test email from Ara Chat.".into(),
                timestamp: Utc::now().format("%H:%M").to_string(),
                is_mention: false,
            }],
            unread_count: 1,
            app_url: self.templates.app_url.clone(),
            conversation_url: format!("{}/chat/test", self.templates.app_url),
            unsubscribe_url: self.templates.unsubscribe_url.clone(),
        };

        let email = self.templates.render(
            EmailType::Message,
            &context,
            to_address,
            Some("Test User".into()),
        )?;

        self.backend.send(&email).await
    }

    fn is_quiet_hours(&self, prefs: &EmailPreferences) -> bool {
        if let (Some(start), Some(end)) = (prefs.quiet_hours_start, prefs.quiet_hours_end) {
            let current_hour = Utc::now().hour() as i16;
            if start <= end {
                current_hour >= start && current_hour < end
            } else {
                // Wraps around midnight
                current_hour >= start || current_hour < end
            }
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quiet_hours_normal() {
        let prefs = EmailPreferences {
            quiet_hours_start: Some(22), // 10 PM
            quiet_hours_end: Some(8),    // 8 AM
            ..Default::default()
        };

        // Note: This test depends on the current time
        // In a real test, we'd mock Utc::now()
        let _ = prefs;
    }

    #[test]
    fn test_quiet_hours_none() {
        let prefs = EmailPreferences {
            quiet_hours_start: None,
            quiet_hours_end: None,
            ..Default::default()
        };

        // Without quiet hours, should never be in quiet period
        // (tested via service.is_quiet_hours)
        let _ = prefs;
    }

    #[test]
    fn test_config_creation() {
        let config = EmailServiceConfig {
            enabled: true,
            delay_seconds: 120,
            max_batch_size: 10,
            batch_window_seconds: 300,
            max_emails_per_hour: 5,
        };

        assert!(config.enabled);
        assert_eq!(config.delay_seconds, 120);
    }
}
