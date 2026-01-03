//! Content sanitization for XSS prevention

use html_escape::encode_text;

/// Sanitize message content to prevent XSS attacks.
///
/// Encodes HTML special characters:
/// - `<` → `&lt;`
/// - `>` → `&gt;`
/// - `&` → `&amp;`
/// - `"` → `&quot;`
/// - `'` → `&#x27;`
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
}
