//! Email notification support
//!
//! Provides email notifications for offline users when they receive
//! new messages or @mentions.
//!
//! # Features
//!
//! - SMTP and SendGrid backends
//! - Configurable delay (avoid emails for quick reconnects)
//! - Message batching (combine multiple messages in one email)
//! - User preferences (opt-out, quiet hours)
//! - Rate limiting (max emails per hour)
//!
//! # Configuration
//!
//! Set environment variables:
//!
//! ```text
//! CHAT__EMAIL__ENABLED=true
//! CHAT__EMAIL__BACKEND=smtp  # or sendgrid
//! CHAT__EMAIL__FROM_ADDRESS=notifications@example.com
//! CHAT__EMAIL__SMTP_HOST=smtp.gmail.com
//! CHAT__EMAIL__SMTP_PORT=587
//! ```

pub mod backends;
mod error;
mod queue;
mod service;
mod templates;
mod types;

pub use error::EmailError;
pub use queue::EmailQueue;
pub use service::{EmailService, EmailServiceConfig};
pub use templates::EmailTemplates;
pub use types::*;
