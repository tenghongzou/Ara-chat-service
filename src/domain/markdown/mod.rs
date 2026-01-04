//! Markdown parsing and rendering hints
//!
//! Provides markdown content analysis for client-side rendering.
//! Instead of clients parsing markdown themselves, the server extracts
//! position-based formatting hints during message send.
//!
//! # Usage
//!
//! ```
//! use ara_chat_service::markdown::{parse_markdown, might_contain_markdown};
//!
//! let content = "Hello **world** and `code`!";
//!
//! // Quick check first
//! if might_contain_markdown(content) {
//!     let hints = parse_markdown(content);
//!     if hints.has_formatting {
//!         // hints.spans contains position-based formatting info
//!     }
//! }
//! ```
//!
//! # Supported Markdown Elements
//!
//! | Element | Syntax |
//! |---------|--------|
//! | Bold | `**text**` or `__text__` |
//! | Italic | `*text*` or `_text_` |
//! | Inline Code | `` `code` `` |
//! | Code Block | ```` ``` ```` |
//! | Link | `[text](url)` |
//! | Strikethrough | `~~text~~` |
//! | Heading | `# text` |
//! | Blockquote | `> text` |
//! | List Item | `- item` or `1. item` |

mod parser;
mod types;

pub use parser::{might_contain_markdown, parse_markdown};
pub use types::{MarkdownSpan, RenderingHints, SpanType};
