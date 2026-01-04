//! Markdown rendering hint types
//!
//! Types for representing markdown formatting information that clients
//! can use to render rich text without parsing markdown themselves.

use serde::{Deserialize, Serialize};

/// A markdown formatting span with position information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarkdownSpan {
    /// Start byte offset in the content string
    pub start: usize,
    /// End byte offset in the content string (exclusive)
    pub end: usize,
    /// Type of markdown formatting
    #[serde(rename = "type")]
    pub span_type: SpanType,
}

/// Types of markdown formatting
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SpanType {
    /// Bold text (**text** or __text__)
    Bold,
    /// Italic text (*text* or _text_)
    Italic,
    /// Bold and italic combined (***text***)
    BoldItalic,
    /// Inline code (`code`)
    Code,
    /// Fenced code block (```language ... ```)
    CodeBlock {
        /// Programming language hint (if specified)
        language: Option<String>,
    },
    /// Hyperlink ([text](url))
    Link {
        /// Target URL
        url: String,
    },
    /// Strikethrough text (~~text~~)
    Strikethrough,
    /// Heading (# to ######)
    Heading {
        /// Heading level (1-6)
        level: u8,
    },
    /// Block quote (> text)
    BlockQuote,
    /// List item (- item or 1. item)
    ListItem {
        /// Whether this is an ordered (numbered) list
        ordered: bool,
    },
}

/// Rendering hints for a message
///
/// Contains information about markdown formatting in message content
/// that clients can use for rich text rendering.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RenderingHints {
    /// List of markdown spans in the content
    pub spans: Vec<MarkdownSpan>,
    /// Whether content contains any markdown formatting
    pub has_formatting: bool,
}

impl RenderingHints {
    /// Create empty rendering hints (no formatting)
    pub fn empty() -> Self {
        Self {
            spans: Vec::new(),
            has_formatting: false,
        }
    }

    /// Check if there are no formatting hints
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rendering_hints_empty() {
        let hints = RenderingHints::empty();
        assert!(!hints.has_formatting);
        assert!(hints.is_empty());
    }

    #[test]
    fn test_rendering_hints_with_spans() {
        let hints = RenderingHints {
            spans: vec![MarkdownSpan {
                start: 0,
                end: 10,
                span_type: SpanType::Bold,
            }],
            has_formatting: true,
        };
        assert!(hints.has_formatting);
        assert!(!hints.is_empty());
    }

    #[test]
    fn test_span_type_serialization() {
        let span = MarkdownSpan {
            start: 0,
            end: 5,
            span_type: SpanType::Bold,
        };
        let json = serde_json::to_string(&span).unwrap();
        assert!(json.contains("\"type\":\"bold\""));
    }

    #[test]
    fn test_code_block_serialization() {
        let span = MarkdownSpan {
            start: 0,
            end: 20,
            span_type: SpanType::CodeBlock {
                language: Some("rust".to_string()),
            },
        };
        let json = serde_json::to_string(&span).unwrap();
        // Struct variants are serialized with the variant name as a key
        assert!(json.contains("code_block"));
        assert!(json.contains("\"language\":\"rust\""));
    }

    #[test]
    fn test_link_serialization() {
        let span = MarkdownSpan {
            start: 0,
            end: 30,
            span_type: SpanType::Link {
                url: "https://example.com".to_string(),
            },
        };
        let json = serde_json::to_string(&span).unwrap();
        // Struct variants are serialized with the variant name as a key
        assert!(json.contains("link"));
        assert!(json.contains("\"url\":\"https://example.com\""));
    }

    #[test]
    fn test_heading_serialization() {
        let span = MarkdownSpan {
            start: 0,
            end: 10,
            span_type: SpanType::Heading { level: 2 },
        };
        let json = serde_json::to_string(&span).unwrap();
        // Struct variants are serialized with the variant name as a key
        assert!(json.contains("heading"));
        assert!(json.contains("\"level\":2"));
    }
}
