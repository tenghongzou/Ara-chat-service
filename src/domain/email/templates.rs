//! Email template rendering

use crate::config::EmailTemplateSettings;

use super::error::EmailError;
use super::types::{EmailMessage, EmailType, NotificationContext};

/// Email template renderer
pub struct EmailTemplates {
    pub app_name: String,
    pub app_url: String,
    pub logo_url: Option<String>,
    pub unsubscribe_url: Option<String>,
}

impl EmailTemplates {
    /// Create a new template renderer from settings
    pub fn new(settings: &EmailTemplateSettings) -> Self {
        Self {
            app_name: settings.app_name.clone(),
            app_url: settings.app_url.clone(),
            logo_url: settings.logo_url.clone(),
            unsubscribe_url: settings.unsubscribe_url.clone(),
        }
    }

    /// Render email template
    pub fn render(
        &self,
        email_type: EmailType,
        context: &NotificationContext,
        to_address: &str,
        to_name: Option<String>,
    ) -> Result<EmailMessage, EmailError> {
        let subject = self.render_subject(email_type, context);
        let html = self.render_html(email_type, context);
        let text = self.render_text(email_type, context);

        Ok(EmailMessage {
            to_address: to_address.to_string(),
            to_name,
            subject,
            html_body: html,
            text_body: text,
        })
    }

    fn render_subject(&self, email_type: EmailType, ctx: &NotificationContext) -> String {
        match email_type {
            EmailType::Mention => {
                format!(
                    "You were mentioned in {} - {}",
                    ctx.conversation_name
                        .as_deref()
                        .unwrap_or("a conversation"),
                    self.app_name
                )
            }
            EmailType::Message => {
                if ctx.unread_count == 1 {
                    format!(
                        "New message in {} - {}",
                        ctx.conversation_name
                            .as_deref()
                            .unwrap_or("a conversation"),
                        self.app_name
                    )
                } else {
                    format!(
                        "{} new messages in {} - {}",
                        ctx.unread_count,
                        ctx.conversation_name
                            .as_deref()
                            .unwrap_or("a conversation"),
                        self.app_name
                    )
                }
            }
            EmailType::Digest => {
                format!("Your {} message digest", self.app_name)
            }
        }
    }

    fn render_html(&self, email_type: EmailType, ctx: &NotificationContext) -> String {
        let messages_html: String = ctx
            .messages
            .iter()
            .map(|msg| {
                format!(
                    r#"
            <div style="margin-bottom: 16px; padding: 12px; background: #f5f5f5; border-radius: 8px;">
                <div style="font-weight: bold; color: #333;">{}</div>
                <div style="color: #666; margin-top: 4px;">{}</div>
                <div style="color: #999; font-size: 12px; margin-top: 4px;">{}</div>
            </div>
            "#,
                    html_escape::encode_text(&msg.sender_name),
                    html_escape::encode_text(&msg.content),
                    msg.timestamp
                )
            })
            .collect();

        let greeting = ctx
            .user_name
            .as_ref()
            .map(|n| format!("Hi {},", n))
            .unwrap_or_else(|| "Hi,".to_string());

        let intro = match email_type {
            EmailType::Mention => "You were mentioned in a conversation:",
            EmailType::Message => "You have new messages:",
            EmailType::Digest => "Here's your message digest:",
        };

        let logo_html = self
            .logo_url
            .as_ref()
            .map(|url| {
                format!(
                    r#"<img src="{}" alt="{}" style="max-height: 40px; margin-bottom: 24px;">"#,
                    url, self.app_name
                )
            })
            .unwrap_or_default();

        let unsubscribe_html = ctx
            .unsubscribe_url
            .as_ref()
            .map(|url| {
                format!(
                    r#"<a href="{}" style="color: #999;">Manage notification preferences</a>"#,
                    url
                )
            })
            .unwrap_or_default();

        format!(
            r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
</head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 0; padding: 20px; background: #f0f0f0;">
    <div style="max-width: 600px; margin: 0 auto; background: white; border-radius: 12px; padding: 32px;">
        {logo}
        <h2 style="color: #333; margin-bottom: 8px;">{greeting}</h2>
        <p style="color: #666;">{intro}</p>

        {messages}

        <a href="{conversation_url}" style="display: inline-block; background: #007bff; color: white; padding: 12px 24px; border-radius: 6px; text-decoration: none; margin-top: 16px;">
            View Conversation
        </a>

        <hr style="border: none; border-top: 1px solid #eee; margin: 32px 0;">

        <p style="color: #999; font-size: 12px;">
            This email was sent by {app_name}.<br>
            {unsubscribe}
        </p>
    </div>
</body>
</html>
        "#,
            logo = logo_html,
            greeting = greeting,
            intro = intro,
            messages = messages_html,
            conversation_url = ctx.conversation_url,
            app_name = self.app_name,
            unsubscribe = unsubscribe_html
        )
    }

    fn render_text(&self, email_type: EmailType, ctx: &NotificationContext) -> String {
        let messages_text: String = ctx
            .messages
            .iter()
            .map(|msg| format!("{}: {}\n  at {}\n\n", msg.sender_name, msg.content, msg.timestamp))
            .collect();

        let greeting = ctx
            .user_name
            .as_ref()
            .map(|n| format!("Hi {},\n\n", n))
            .unwrap_or_else(|| "Hi,\n\n".to_string());

        let intro = match email_type {
            EmailType::Mention => "You were mentioned in a conversation:\n\n",
            EmailType::Message => "You have new messages:\n\n",
            EmailType::Digest => "Here's your message digest:\n\n",
        };

        format!(
            "{greeting}{intro}{messages}View the conversation: {url}\n\n---\nThis email was sent by {app}.",
            greeting = greeting,
            intro = intro,
            messages = messages_text,
            url = ctx.conversation_url,
            app = self.app_name
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::email::types::MessagePreview;

    fn create_test_context() -> NotificationContext {
        NotificationContext {
            user_name: Some("Test User".into()),
            conversation_name: Some("Team Chat".into()),
            messages: vec![MessagePreview {
                sender_name: "Alice".into(),
                content: "Hello there!".into(),
                timestamp: "10:30 AM".into(),
                is_mention: false,
            }],
            unread_count: 1,
            app_url: "https://app.example.com".into(),
            conversation_url: "https://app.example.com/chat/123".into(),
            unsubscribe_url: Some("https://app.example.com/unsubscribe".into()),
        }
    }

    #[test]
    fn test_render_subject_message() {
        let templates = EmailTemplates {
            app_name: "TestApp".into(),
            app_url: "https://test.com".into(),
            logo_url: None,
            unsubscribe_url: None,
        };

        let ctx = create_test_context();
        let subject = templates.render_subject(EmailType::Message, &ctx);
        assert!(subject.contains("Team Chat"));
        assert!(subject.contains("TestApp"));
    }

    #[test]
    fn test_render_subject_mention() {
        let templates = EmailTemplates {
            app_name: "TestApp".into(),
            app_url: "https://test.com".into(),
            logo_url: None,
            unsubscribe_url: None,
        };

        let ctx = create_test_context();
        let subject = templates.render_subject(EmailType::Mention, &ctx);
        assert!(subject.contains("mentioned"));
    }

    #[test]
    fn test_render_html_contains_content() {
        let templates = EmailTemplates {
            app_name: "TestApp".into(),
            app_url: "https://test.com".into(),
            logo_url: None,
            unsubscribe_url: None,
        };

        let ctx = create_test_context();
        let html = templates.render_html(EmailType::Message, &ctx);

        assert!(html.contains("Test User"));
        assert!(html.contains("Hello there!"));
        assert!(html.contains("Alice"));
        assert!(html.contains("View Conversation"));
    }

    #[test]
    fn test_render_text_contains_content() {
        let templates = EmailTemplates {
            app_name: "TestApp".into(),
            app_url: "https://test.com".into(),
            logo_url: None,
            unsubscribe_url: None,
        };

        let ctx = create_test_context();
        let text = templates.render_text(EmailType::Message, &ctx);

        assert!(text.contains("Test User"));
        assert!(text.contains("Hello there!"));
        assert!(text.contains("Alice"));
    }

    #[test]
    fn test_render_full_email() {
        let templates = EmailTemplates {
            app_name: "TestApp".into(),
            app_url: "https://test.com".into(),
            logo_url: None,
            unsubscribe_url: None,
        };

        let ctx = create_test_context();
        let email = templates
            .render(
                EmailType::Message,
                &ctx,
                "user@example.com",
                Some("Test User".into()),
            )
            .unwrap();

        assert_eq!(email.to_address, "user@example.com");
        assert!(email.subject.contains("Team Chat"));
        assert!(!email.html_body.is_empty());
        assert!(!email.text_body.is_empty());
    }
}
