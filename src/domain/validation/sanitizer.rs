//! Content sanitization for XSS prevention

use html_escape::encode_text;

/// Sanitize message content to prevent XSS attacks.
///
/// Encodes HTML special characters:
/// - `<` → `&lt;`
/// - `>` → `&gt;`
/// - `&` → `&amp;`
///
/// Note: Single and double quotes are NOT encoded by `encode_text`.
/// This is sufficient for text content but not for attribute values.
pub fn sanitize_message_content(content: &str) -> String {
    encode_text(content).into_owned()
}

/// Sanitize conversation name
pub fn sanitize_conversation_name(name: &str) -> String {
    encode_text(name).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_script_tag() {
        let input = "<script>alert('xss')</script>";
        let output = sanitize_message_content(input);
        assert!(!output.contains('<'));
        assert!(!output.contains('>'));
        assert!(output.contains("&lt;"));
        assert!(output.contains("&gt;"));
    }

    #[test]
    fn test_sanitize_html_attributes() {
        let input = r#"<img src="x" onerror="alert('xss')">"#;
        let output = sanitize_message_content(input);
        assert!(!output.contains('<'));
        assert!(output.contains("&lt;"));
    }

    #[test]
    fn test_preserves_normal_text() {
        let input = "Hello, how are you?";
        let output = sanitize_message_content(input);
        assert_eq!(input, output);
    }

    #[test]
    fn test_preserves_unicode() {
        let input = "你好世界 🎉 こんにちは";
        let output = sanitize_message_content(input);
        assert_eq!(input, output);
    }

    #[test]
    fn test_sanitize_ampersand() {
        let input = "Tom & Jerry";
        let output = sanitize_message_content(input);
        assert!(output.contains("&amp;"));
    }

    #[test]
    fn test_sanitize_null_bytes() {
        let input = "hello\0world";
        let output = sanitize_message_content(input);
        // Null bytes should be preserved (not HTML special)
        assert!(output.contains('\0'));
    }

    #[test]
    fn test_sanitize_javascript_protocol() {
        let input = "javascript:alert('xss')";
        let output = sanitize_message_content(input);
        // javascript: URI is preserved (not HTML special character)
        // Note: encode_text only encodes <, >, & - single quotes are preserved
        assert!(output.contains("javascript:"));
        assert!(output.contains("alert"));
        // Single quotes are preserved by encode_text (not encoded to &#x27;)
        assert!(output.contains("'"));
    }

    #[test]
    fn test_sanitize_data_uri() {
        let input = "data:text/html,<script>alert('xss')</script>";
        let output = sanitize_message_content(input);
        assert!(!output.contains('<'));
        assert!(output.contains("&lt;"));
    }

    #[test]
    fn test_sanitize_max_length_content() {
        let input = "a".repeat(10_000);
        let output = sanitize_message_content(&input);
        assert_eq!(output.len(), 10_000);
    }

    #[test]
    fn test_sanitize_empty_string() {
        let input = "";
        let output = sanitize_message_content(input);
        assert_eq!(output, "");
    }

    #[test]
    fn test_sanitize_only_whitespace() {
        let input = "   \t\n\r  ";
        let output = sanitize_message_content(input);
        assert_eq!(input, output);
    }

    #[test]
    fn test_sanitize_double_encoded() {
        // Already encoded content should be encoded again
        let input = "&lt;script&gt;";
        let output = sanitize_message_content(input);
        assert!(output.contains("&amp;lt;"));
    }

    #[test]
    fn test_sanitize_unicode_rtl_override() {
        let input = "\u{202E}evil";
        let output = sanitize_message_content(input);
        // RTL override character is preserved (not HTML)
        assert!(output.contains('\u{202E}'));
    }

    #[test]
    fn test_sanitize_mixed_content() {
        let input = "Hello <b>world</b> & goodbye!";
        let output = sanitize_message_content(input);
        assert!(!output.contains('<'));
        assert!(!output.contains('>'));
        assert!(output.contains("&lt;b&gt;"));
        assert!(output.contains("&amp;"));
    }

    #[test]
    fn test_sanitize_conversation_name_special_chars() {
        let input = "Team <Developers> & Friends";
        let output = sanitize_conversation_name(input);
        assert!(!output.contains('<'));
        assert!(!output.contains('>'));
        assert!(output.contains("&lt;"));
        assert!(output.contains("&gt;"));
        assert!(output.contains("&amp;"));
    }
}
