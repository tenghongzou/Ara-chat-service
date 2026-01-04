//! SendGrid HTTP API backend

use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;

use super::EmailSender;
use crate::domain::email::error::EmailError;
use crate::domain::email::types::EmailMessage;

/// SendGrid backend for sending emails via HTTP API
pub struct SendGridBackend {
    client: Client,
    api_key: String,
    from_address: String,
    from_name: String,
}

#[derive(Serialize)]
struct SendGridRequest {
    personalizations: Vec<Personalization>,
    from: EmailAddress,
    subject: String,
    content: Vec<Content>,
}

#[derive(Serialize)]
struct Personalization {
    to: Vec<EmailAddress>,
}

#[derive(Serialize)]
struct EmailAddress {
    email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Serialize)]
struct Content {
    #[serde(rename = "type")]
    content_type: String,
    value: String,
}

impl SendGridBackend {
    /// Create a new SendGrid backend
    pub fn new(api_key: String, from_address: String, from_name: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            from_address,
            from_name,
        }
    }
}

#[async_trait]
impl EmailSender for SendGridBackend {
    async fn send(&self, email: &EmailMessage) -> Result<(), EmailError> {
        let request = SendGridRequest {
            personalizations: vec![Personalization {
                to: vec![EmailAddress {
                    email: email.to_address.clone(),
                    name: email.to_name.clone(),
                }],
            }],
            from: EmailAddress {
                email: self.from_address.clone(),
                name: Some(self.from_name.clone()),
            },
            subject: email.subject.clone(),
            content: vec![
                Content {
                    content_type: "text/plain".to_string(),
                    value: email.text_body.clone(),
                },
                Content {
                    content_type: "text/html".to_string(),
                    value: email.html_body.clone(),
                },
            ],
        };

        let response = self
            .client
            .post("https://api.sendgrid.com/v3/mail/send")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(EmailError::SendGrid(format!(
                "SendGrid API error {}: {}",
                status, body
            )));
        }

        Ok(())
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn name(&self) -> &'static str {
        "sendgrid"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sendgrid_backend_name() {
        let backend = SendGridBackend::new(
            "test-api-key".into(),
            "test@example.com".into(),
            "Test".into(),
        );

        assert_eq!(backend.name(), "sendgrid");
        assert!(backend.is_available());
    }

    #[test]
    fn test_sendgrid_backend_unavailable_with_empty_key() {
        let backend =
            SendGridBackend::new("".into(), "test@example.com".into(), "Test".into());

        assert!(!backend.is_available());
    }
}
