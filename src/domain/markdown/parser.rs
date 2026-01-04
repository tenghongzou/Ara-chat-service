//! Markdown parsing and rendering hint extraction
//!
//! Parses markdown content using pulldown-cmark and extracts position-based
//! formatting hints for client-side rendering.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use super::types::{MarkdownSpan, RenderingHints, SpanType};

/// Maximum number of spans to return per message
const MAX_SPANS_PER_MESSAGE: usize = 100;

/// Parse markdown content and extract rendering hints
///
/// Uses pulldown-cmark to parse CommonMark-compliant markdown and extracts
/// position-based spans that clients can use for rich text rendering.
pub fn parse_markdown(content: &str) -> RenderingHints {
    // Enable strikethrough extension
    let options = Options::ENABLE_STRIKETHROUGH;
    let parser = Parser::new_ext(content, options);
    let mut spans = Vec::new();
    let mut context_stack: Vec<(usize, ContextType)> = Vec::new();

    for (event, range) in parser.into_offset_iter() {
        if spans.len() >= MAX_SPANS_PER_MESSAGE {
            break;
        }

        match event {
            Event::Start(tag) => {
                if let Some(ctx) = tag_to_context(&tag) {
                    context_stack.push((range.start, ctx));
                }
            }
            Event::End(tag_end) => {
                if let Some((start, ctx)) = pop_matching_context(&mut context_stack, &tag_end) {
                    if let Some(span_type) = ctx.to_span_type() {
                        spans.push(MarkdownSpan {
                            start,
                            end: range.end,
                            span_type,
                        });
                    }
                }
            }
            Event::Code(_) => {
                // Inline code is self-contained
                spans.push(MarkdownSpan {
                    start: range.start,
                    end: range.end,
                    span_type: SpanType::Code,
                });
            }
            _ => {}
        }
    }

    // Sort spans by start position for consistent output
    spans.sort_by_key(|s| s.start);

    RenderingHints {
        has_formatting: !spans.is_empty(),
        spans,
    }
}

/// Quick check if content might contain markdown
///
/// This is a fast pre-filter to avoid parsing content that definitely
/// has no markdown. Returns false only if content cannot contain markdown.
pub fn might_contain_markdown(content: &str) -> bool {
    content.contains('*')
        || content.contains('_')
        || content.contains('`')
        || content.contains('[')
        || content.contains('#')
        || content.contains('>')
        || content.contains('~')
        || (content.contains('-') && content.contains('\n'))
        || (content.contains("1.") && content.contains('\n'))
}

/// Context information during parsing
#[derive(Debug, Clone)]
enum ContextType {
    Strong,
    Emphasis,
    Strikethrough,
    Link { url: String },
    CodeBlock { language: Option<String> },
    Heading { level: u8 },
    BlockQuote,
    List { ordered: bool },
    Item { ordered: bool },
}

impl ContextType {
    fn to_span_type(self) -> Option<SpanType> {
        match self {
            ContextType::Strong => Some(SpanType::Bold),
            ContextType::Emphasis => Some(SpanType::Italic),
            ContextType::Strikethrough => Some(SpanType::Strikethrough),
            ContextType::Link { url } => Some(SpanType::Link { url }),
            ContextType::CodeBlock { language } => Some(SpanType::CodeBlock { language }),
            ContextType::Heading { level } => Some(SpanType::Heading { level }),
            ContextType::BlockQuote => Some(SpanType::BlockQuote),
            ContextType::Item { ordered } => Some(SpanType::ListItem { ordered }),
            ContextType::List { .. } => None, // List container doesn't emit span
        }
    }
}

fn tag_to_context(tag: &Tag) -> Option<ContextType> {
    match tag {
        Tag::Strong => Some(ContextType::Strong),
        Tag::Emphasis => Some(ContextType::Emphasis),
        Tag::Strikethrough => Some(ContextType::Strikethrough),
        Tag::Link { dest_url, .. } => Some(ContextType::Link {
            url: dest_url.to_string(),
        }),
        Tag::CodeBlock(kind) => {
            let language = match kind {
                CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(lang.to_string()),
                _ => None,
            };
            Some(ContextType::CodeBlock { language })
        }
        Tag::Heading { level, .. } => Some(ContextType::Heading {
            level: *level as u8,
        }),
        Tag::BlockQuote(_) => Some(ContextType::BlockQuote),
        Tag::List(first_number) => Some(ContextType::List {
            ordered: first_number.is_some(),
        }),
        Tag::Item => {
            // Look for parent list to determine if ordered
            Some(ContextType::Item { ordered: false })
        }
        _ => None,
    }
}

fn pop_matching_context(
    stack: &mut Vec<(usize, ContextType)>,
    tag_end: &TagEnd,
) -> Option<(usize, ContextType)> {
    let matches = |ctx: &ContextType| -> bool {
        matches!(
            (ctx, tag_end),
            (ContextType::Strong, TagEnd::Strong)
                | (ContextType::Emphasis, TagEnd::Emphasis)
                | (ContextType::Strikethrough, TagEnd::Strikethrough)
                | (ContextType::Link { .. }, TagEnd::Link)
                | (ContextType::CodeBlock { .. }, TagEnd::CodeBlock)
                | (ContextType::Heading { .. }, TagEnd::Heading(_))
                | (ContextType::BlockQuote, TagEnd::BlockQuote(_))
                | (ContextType::List { .. }, TagEnd::List(_))
                | (ContextType::Item { .. }, TagEnd::Item)
        )
    };

    // Find and remove matching context from stack
    for i in (0..stack.len()).rev() {
        if matches(&stack[i].1) {
            return Some(stack.remove(i));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bold() {
        let hints = parse_markdown("Hello **world**!");
        assert!(hints.has_formatting);
        assert_eq!(hints.spans.len(), 1);
        assert_eq!(hints.spans[0].span_type, SpanType::Bold);
        assert_eq!(hints.spans[0].start, 6);
        assert_eq!(hints.spans[0].end, 15);
    }

    #[test]
    fn test_parse_italic() {
        let hints = parse_markdown("Hello *world*!");
        assert!(hints.has_formatting);
        assert_eq!(hints.spans.len(), 1);
        assert_eq!(hints.spans[0].span_type, SpanType::Italic);
    }

    #[test]
    fn test_parse_inline_code() {
        let hints = parse_markdown("Use `code` here");
        assert!(hints.has_formatting);
        assert_eq!(hints.spans.len(), 1);
        assert_eq!(hints.spans[0].span_type, SpanType::Code);
    }

    #[test]
    fn test_parse_code_block_with_language() {
        let content = "```rust\nfn main() {}\n```";
        let hints = parse_markdown(content);
        assert!(hints.has_formatting);
        assert_eq!(hints.spans.len(), 1);
        match &hints.spans[0].span_type {
            SpanType::CodeBlock { language } => {
                assert_eq!(language.as_deref(), Some("rust"));
            }
            _ => panic!("Expected CodeBlock"),
        }
    }

    #[test]
    fn test_parse_code_block_without_language() {
        let content = "```\ncode here\n```";
        let hints = parse_markdown(content);
        assert!(hints.has_formatting);
        assert_eq!(hints.spans.len(), 1);
        match &hints.spans[0].span_type {
            SpanType::CodeBlock { language } => {
                assert!(language.is_none());
            }
            _ => panic!("Expected CodeBlock"),
        }
    }

    #[test]
    fn test_parse_link() {
        let hints = parse_markdown("Click [here](https://example.com)!");
        assert!(hints.has_formatting);
        assert_eq!(hints.spans.len(), 1);
        match &hints.spans[0].span_type {
            SpanType::Link { url } => {
                assert_eq!(url, "https://example.com");
            }
            _ => panic!("Expected Link"),
        }
    }

    #[test]
    fn test_parse_strikethrough() {
        let hints = parse_markdown("~~deleted~~");
        assert!(hints.has_formatting);
        assert_eq!(hints.spans.len(), 1);
        assert_eq!(hints.spans[0].span_type, SpanType::Strikethrough);
    }

    #[test]
    fn test_parse_heading() {
        let hints = parse_markdown("# Title\n\nContent");
        assert!(hints.has_formatting);
        assert!(!hints.spans.is_empty());
        let heading = hints
            .spans
            .iter()
            .find(|s| matches!(s.span_type, SpanType::Heading { .. }));
        assert!(heading.is_some());
        match &heading.unwrap().span_type {
            SpanType::Heading { level } => assert_eq!(*level, 1),
            _ => panic!("Expected Heading"),
        }
    }

    #[test]
    fn test_parse_blockquote() {
        let hints = parse_markdown("> Quote here");
        assert!(hints.has_formatting);
        let blockquote = hints
            .spans
            .iter()
            .find(|s| matches!(s.span_type, SpanType::BlockQuote));
        assert!(blockquote.is_some());
    }

    #[test]
    fn test_parse_multiple_formats() {
        let hints = parse_markdown("**bold** and *italic* and `code`");
        assert!(hints.has_formatting);
        assert_eq!(hints.spans.len(), 3);
    }

    #[test]
    fn test_no_markdown() {
        let hints = parse_markdown("Hello world!");
        assert!(!hints.has_formatting);
        assert!(hints.spans.is_empty());
    }

    #[test]
    fn test_might_contain_markdown_true() {
        assert!(might_contain_markdown("**bold**"));
        assert!(might_contain_markdown("*italic*"));
        assert!(might_contain_markdown("`code`"));
        assert!(might_contain_markdown("[link](url)"));
        assert!(might_contain_markdown("# heading"));
        assert!(might_contain_markdown("> quote"));
        assert!(might_contain_markdown("~~strike~~"));
    }

    #[test]
    fn test_might_contain_markdown_false() {
        assert!(!might_contain_markdown("plain text"));
        assert!(!might_contain_markdown("Hello world!"));
        assert!(!might_contain_markdown("No special chars here"));
    }

    #[test]
    fn test_max_spans_limit() {
        // Generate content with many markdown elements
        let content = (0..150)
            .map(|i| format!("**bold{}**", i))
            .collect::<Vec<_>>()
            .join(" ");
        let hints = parse_markdown(&content);
        assert!(hints.spans.len() <= MAX_SPANS_PER_MESSAGE);
    }

    #[test]
    fn test_nested_formatting() {
        let hints = parse_markdown("***bold italic***");
        assert!(hints.has_formatting);
        // Should capture both strong and emphasis
        assert!(hints.spans.len() >= 1);
    }

    #[test]
    fn test_list_items() {
        let content = "- item 1\n- item 2";
        let hints = parse_markdown(content);
        assert!(hints.has_formatting);
        let items: Vec<_> = hints
            .spans
            .iter()
            .filter(|s| matches!(s.span_type, SpanType::ListItem { .. }))
            .collect();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_spans_sorted_by_position() {
        let hints = parse_markdown("`code` and **bold**");
        assert!(hints.has_formatting);
        for i in 1..hints.spans.len() {
            assert!(hints.spans[i - 1].start <= hints.spans[i].start);
        }
    }
}
