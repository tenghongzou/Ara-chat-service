//! SMTP email backend using lettre

use async_trait::async_trait;
use lettre::message::{header::ContentType, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use super::EmailSender;
use crate::domain::email::error::EmailError;
use crate::domain::email::types::EmailMessage;

/// SMTP backend for sending emails
pub struct SmtpBackend {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from_address: String,
    from_name: String,
}

impl SmtpBackend {
    /// Create a new SMTP backend
    pub fn new(
        host: String,
        port: u16,
        username: Option<String>,
        password: Option<String>,
        use_tls: bool,
        from_address: String,
        from_name: String,
    ) -> Result<Self, EmailError> {
        let mut builder = if use_tls {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)
                .map_err(|e| EmailError::Smtp(format!("Failed to create SMTP relay: {}", e)))?
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host)
        };

        builder = builder.port(port);

        if let (Some(user), Some(pass)) = (username, password) {
            builder = builder.credentials(Credentials::new(user, pass));
        }

        Ok(Self {
            transport: builder.build(),
            from_address,
            from_name,
        })
    }
}

#[async_trait]
impl EmailSender for SmtpBackend {
    async fn send(&self, email: &EmailMessage) -> Result<(), EmailError> {
        let from = format!("{} <{}>", self.from_name, self.from_address);
        let to = match &email.to_name {
            Some(name) => format!("{} <{}>", name, email.to_address),
            None => email.to_address.clone(),
        };

        let message = Message::builder()
            .from(
                from.parse()
                    .map_err(|e| EmailError::Smtp(format!("Invalid from address: {}", e)))?,
            )
            .to(to
                .parse()
                .map_err(|e| EmailError::Smtp(format!("Invalid to address: {}", e)))?)
            .subject(&email.subject)
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_PLAIN)
                            .body(email.text_body.clone()),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(email.html_body.clone()),
                    ),
            )
            .map_err(|e| EmailError::Smtp(format!("Failed to build email: {}", e)))?;

        self.transport
            .send(message)
            .await
            .map_err(|e| EmailError::Smtp(format!("Failed to send email: {}", e)))?;

        Ok(())
    }

    fn is_available(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "smtp"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smtp_backend_name() {
        // Can't easily test SMTP without a server, but we can test the trait impl
        let backend = SmtpBackend::new(
            "localhost".into(),
            587,
            None,
            None,
            false,
            "test@example.com".into(),
            "Test".into(),
        )
        .unwrap();

        assert_eq!(backend.name(), "smtp");
        assert!(backend.is_available());
    }
}
