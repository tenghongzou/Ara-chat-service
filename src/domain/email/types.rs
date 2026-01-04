//! Email notification types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Email notification type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailType {
    Message,
    Mention,
    Digest,
}

impl EmailType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::Mention => "mention",
            Self::Digest => "digest",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "message" => Some(Self::Message),
            "mention" => Some(Self::Mention),
            "digest" => Some(Self::Digest),
            _ => None,
        }
    }
}

/// Email priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EmailPriority {
    Low,
    #[default]
    Normal,
    High,
}

impl EmailPriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "low" => Some(Self::Low),
            "normal" => Some(Self::Normal),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

/// Email queue status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EmailStatus {
    #[default]
    Pending,
    Processing,
    Sent,
    Failed,
    Cancelled,
}

impl EmailStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Sent => "sent",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "processing" => Some(Self::Processing),
            "sent" => Some(Self::Sent),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// Digest mode for email preferences
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DigestMode {
    #[default]
    Immediate,
    Hourly,
    Daily,
}

impl DigestMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::Hourly => "hourly",
            Self::Daily => "daily",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "immediate" => Some(Self::Immediate),
            "hourly" => Some(Self::Hourly),
            "daily" => Some(Self::Daily),
            _ => None,
        }
    }
}

/// User email preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailPreferences {
    pub user_id: Uuid,
    pub email_address: Option<String>,
    pub email_enabled: bool,
    pub notify_messages: bool,
    pub notify_mentions: bool,
    pub digest_mode: DigestMode,
    pub quiet_hours_start: Option<i16>,
    pub quiet_hours_end: Option<i16>,
    pub last_email_sent_at: Option<DateTime<Utc>>,
}

impl Default for EmailPreferences {
    fn default() -> Self {
        Self {
            user_id: Uuid::nil(),
            email_address: None,
            email_enabled: true,
            notify_messages: true,
            notify_mentions: true,
            digest_mode: DigestMode::Immediate,
            quiet_hours_start: None,
            quiet_hours_end: None,
            last_email_sent_at: None,
        }
    }
}

/// Database row for email preferences
#[derive(Debug, FromRow)]
pub struct EmailPreferencesRow {
    pub user_id: Uuid,
    pub email_address: Option<String>,
    pub email_enabled: bool,
    pub notify_messages: bool,
    pub notify_mentions: bool,
    pub digest_mode: String,
    pub quiet_hours_start: Option<i16>,
    pub quiet_hours_end: Option<i16>,
    pub last_email_sent_at: Option<DateTime<Utc>>,
}

impl From<EmailPreferencesRow> for EmailPreferences {
    fn from(row: EmailPreferencesRow) -> Self {
        Self {
            user_id: row.user_id,
            email_address: row.email_address,
            email_enabled: row.email_enabled,
            notify_messages: row.notify_messages,
            notify_mentions: row.notify_mentions,
            digest_mode: DigestMode::from_str(&row.digest_mode).unwrap_or_default(),
            quiet_hours_start: row.quiet_hours_start,
            quiet_hours_end: row.quiet_hours_end,
            last_email_sent_at: row.last_email_sent_at,
        }
    }
}

/// Queued email notification
#[derive(Debug, Clone)]
pub struct QueuedEmail {
    pub id: Uuid,
    pub user_id: Uuid,
    pub email_type: EmailType,
    pub conversation_id: Uuid,
    pub message_ids: Vec<Uuid>,
    pub sender_ids: Vec<Uuid>,
    pub content_previews: Vec<String>,
    pub priority: EmailPriority,
    pub status: EmailStatus,
    pub scheduled_at: DateTime<Utc>,
    pub send_after: DateTime<Utc>,
    pub retry_count: i16,
}

/// Database row for queued email
#[derive(Debug, FromRow)]
pub struct QueuedEmailRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub email_type: String,
    pub conversation_id: Uuid,
    pub message_ids: Vec<Uuid>,
    pub sender_ids: Vec<Uuid>,
    pub content_previews: Option<Vec<String>>,
    pub priority: String,
    pub status: String,
    pub scheduled_at: DateTime<Utc>,
    pub send_after: DateTime<Utc>,
    pub retry_count: i16,
}

impl From<QueuedEmailRow> for QueuedEmail {
    fn from(row: QueuedEmailRow) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            email_type: EmailType::from_str(&row.email_type).unwrap_or(EmailType::Message),
            conversation_id: row.conversation_id,
            message_ids: row.message_ids,
            sender_ids: row.sender_ids,
            content_previews: row.content_previews.unwrap_or_default(),
            priority: EmailPriority::from_str(&row.priority).unwrap_or_default(),
            status: EmailStatus::from_str(&row.status).unwrap_or_default(),
            scheduled_at: row.scheduled_at,
            send_after: row.send_after,
            retry_count: row.retry_count,
        }
    }
}

/// Email to be sent
#[derive(Debug, Clone)]
pub struct EmailMessage {
    pub to_address: String,
    pub to_name: Option<String>,
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
}

/// Notification context for template rendering
#[derive(Debug, Clone, Serialize)]
pub struct NotificationContext {
    pub user_name: Option<String>,
    pub conversation_name: Option<String>,
    pub messages: Vec<MessagePreview>,
    pub unread_count: usize,
    pub app_url: String,
    pub conversation_url: String,
    pub unsubscribe_url: Option<String>,
}

/// Message preview in email
#[derive(Debug, Clone, Serialize)]
pub struct MessagePreview {
    pub sender_name: String,
    pub content: String,
    pub timestamp: String,
    pub is_mention: bool,
}

/// Request to update email preferences
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateEmailPreferencesRequest {
    pub email_address: Option<String>,
    pub email_enabled: Option<bool>,
    pub notify_messages: Option<bool>,
    pub notify_mentions: Option<bool>,
    pub digest_mode: Option<DigestMode>,
    pub quiet_hours_start: Option<i16>,
    pub quiet_hours_end: Option<i16>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_type_conversion() {
        assert_eq!(EmailType::Message.as_str(), "message");
        assert_eq!(EmailType::from_str("mention"), Some(EmailType::Mention));
        assert_eq!(EmailType::from_str("invalid"), None);
    }

    #[test]
    fn test_email_priority_conversion() {
        assert_eq!(EmailPriority::High.as_str(), "high");
        assert_eq!(EmailPriority::from_str("low"), Some(EmailPriority::Low));
    }

    #[test]
    fn test_email_status_conversion() {
        assert_eq!(EmailStatus::Pending.as_str(), "pending");
        assert_eq!(EmailStatus::from_str("sent"), Some(EmailStatus::Sent));
    }

    #[test]
    fn test_digest_mode_conversion() {
        assert_eq!(DigestMode::Immediate.as_str(), "immediate");
        assert_eq!(DigestMode::from_str("hourly"), Some(DigestMode::Hourly));
    }

    #[test]
    fn test_default_preferences() {
        let prefs = EmailPreferences::default();
        assert!(prefs.email_enabled);
        assert!(prefs.notify_messages);
        assert!(prefs.notify_mentions);
        assert_eq!(prefs.digest_mode, DigestMode::Immediate);
    }
}
