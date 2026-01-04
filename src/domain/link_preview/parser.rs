//! URL extraction and Open Graph parsing

use regex_lite::Regex;
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

use super::types::OpenGraphData;

/// Maximum number of URLs to extract from a single message
const MAX_URLS_PER_MESSAGE: usize = 5;

/// Regex pattern for URL extraction
static URL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"https?://[^\s<>\[\]()"'`]+"#).unwrap()
});

/// Extract URLs from message content
///
/// Returns up to MAX_URLS_PER_MESSAGE URLs found in the content.
/// Only extracts http:// and https:// URLs.
pub fn extract_urls(content: &str) -> Vec<String> {
    URL_REGEX
        .find_iter(content)
        .map(|m| {
            let url = m.as_str();
            // Clean trailing punctuation that might have been captured
            url.trim_end_matches(|c| matches!(c, '.' | ',' | ';' | ':' | '!' | '?'))
                .to_string()
        })
        .filter(|url| is_valid_url(url))
        .take(MAX_URLS_PER_MESSAGE)
        .collect()
}

/// Generate SHA256 hash of a URL for deduplication/caching
pub fn url_hash(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Validate URL is safe to fetch
fn is_valid_url(url: &str) -> bool {
    // Must start with http:// or https://
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return false;
    }

    // Parse URL to check host
    if let Ok(parsed) = url::Url::parse(url) {
        // Must have a host
        if parsed.host_str().is_none() {
            return false;
        }

        // Skip localhost and common private domains
        if let Some(host) = parsed.host_str() {
            let host_lower = host.to_lowercase();
            if host_lower == "localhost"
                || host_lower.ends_with(".local")
                || host_lower.ends_with(".internal")
                || host_lower.ends_with(".localhost")
            {
                return false;
            }
        }

        true
    } else {
        false
    }
}

/// Parse HTML content and extract Open Graph metadata
pub fn parse_open_graph(html: &str, base_url: &str) -> OpenGraphData {
    let document = Html::parse_document(html);
    let mut data = OpenGraphData::default();

    // Try to parse base URL for favicon resolution
    let base = url::Url::parse(base_url).ok();

    // Selectors for Open Graph meta tags
    let og_title = Selector::parse("meta[property='og:title']").ok();
    let og_description = Selector::parse("meta[property='og:description']").ok();
    let og_image = Selector::parse("meta[property='og:image']").ok();
    let og_site_name = Selector::parse("meta[property='og:site_name']").ok();
    let og_url = Selector::parse("meta[property='og:url']").ok();

    // Fallback selectors
    let title_tag = Selector::parse("title").ok();
    let meta_description = Selector::parse("meta[name='description']").ok();
    let favicon_link = Selector::parse("link[rel='icon'], link[rel='shortcut icon']").ok();

    // Extract Open Graph title or fallback to <title>
    if let Some(ref sel) = og_title {
        if let Some(el) = document.select(sel).next() {
            data.title = el.value().attr("content").map(|s| truncate(s, 512));
        }
    }
    if data.title.is_none() {
        if let Some(ref sel) = title_tag {
            if let Some(el) = document.select(sel).next() {
                data.title = Some(truncate(&el.text().collect::<String>(), 512));
            }
        }
    }

    // Extract Open Graph description or fallback to meta description
    if let Some(ref sel) = og_description {
        if let Some(el) = document.select(sel).next() {
            data.description = el.value().attr("content").map(|s| truncate(s, 1000));
        }
    }
    if data.description.is_none() {
        if let Some(ref sel) = meta_description {
            if let Some(el) = document.select(sel).next() {
                data.description = el.value().attr("content").map(|s| truncate(s, 1000));
            }
        }
    }

    // Extract Open Graph image
    if let Some(ref sel) = og_image {
        if let Some(el) = document.select(sel).next() {
            if let Some(img) = el.value().attr("content") {
                data.image = resolve_url(img, base.as_ref());
            }
        }
    }

    // Extract site name
    if let Some(ref sel) = og_site_name {
        if let Some(el) = document.select(sel).next() {
            data.site_name = el.value().attr("content").map(|s| truncate(s, 255));
        }
    }

    // Extract canonical URL
    if let Some(ref sel) = og_url {
        if let Some(el) = document.select(sel).next() {
            data.url = el.value().attr("content").map(String::from);
        }
    }

    // Extract favicon
    if let Some(ref sel) = favicon_link {
        if let Some(el) = document.select(sel).next() {
            if let Some(href) = el.value().attr("href") {
                data.favicon = resolve_url(href, base.as_ref());
            }
        }
    }
    // Fallback to default favicon location
    if data.favicon.is_none() {
        if let Some(ref base) = base {
            if let Ok(favicon_url) = base.join("/favicon.ico") {
                data.favicon = Some(favicon_url.to_string());
            }
        }
    }

    data
}

/// Resolve a potentially relative URL to absolute
fn resolve_url(url: &str, base: Option<&url::Url>) -> Option<String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        return Some(url.to_string());
    }

    // Try to resolve relative URL
    if let Some(base) = base {
        if let Ok(resolved) = base.join(url) {
            return Some(resolved.to_string());
        }
    }

    None
}

/// Truncate string to max length, preserving UTF-8 boundaries
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut end = max_len;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_urls_basic() {
        let content = "Check out https://example.com and http://test.org";
        let urls = extract_urls(content);
        assert_eq!(urls.len(), 2);
        assert!(urls.contains(&"https://example.com".to_string()));
        assert!(urls.contains(&"http://test.org".to_string()));
    }

    #[test]
    fn test_extract_urls_with_path() {
        let content = "See https://example.com/path/to/page?query=1";
        let urls = extract_urls(content);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0], "https://example.com/path/to/page?query=1");
    }

    #[test]
    fn test_extract_urls_max_limit() {
        let content = "https://1.com https://2.com https://3.com https://4.com https://5.com https://6.com";
        let urls = extract_urls(content);
        assert_eq!(urls.len(), 5); // MAX_URLS_PER_MESSAGE
    }

    #[test]
    fn test_extract_urls_cleans_punctuation() {
        let content = "Visit https://example.com.";
        let urls = extract_urls(content);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0], "https://example.com");
    }

    #[test]
    fn test_extract_urls_no_localhost() {
        let content = "http://localhost:8080 and https://example.local";
        let urls = extract_urls(content);
        assert!(urls.is_empty());
    }

    #[test]
    fn test_url_hash() {
        let hash = url_hash("https://example.com");
        assert_eq!(hash.len(), 64); // SHA256 hex is 64 chars

        // Same URL should produce same hash
        assert_eq!(hash, url_hash("https://example.com"));

        // Different URL should produce different hash
        assert_ne!(hash, url_hash("https://example.org"));
    }

    #[test]
    fn test_parse_open_graph_basic() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta property="og:title" content="Example Title">
                <meta property="og:description" content="Example description">
                <meta property="og:image" content="https://example.com/image.png">
                <meta property="og:site_name" content="Example Site">
            </head>
            <body></body>
            </html>
        "#;

        let data = parse_open_graph(html, "https://example.com");
        assert_eq!(data.title, Some("Example Title".to_string()));
        assert_eq!(data.description, Some("Example description".to_string()));
        assert_eq!(data.image, Some("https://example.com/image.png".to_string()));
        assert_eq!(data.site_name, Some("Example Site".to_string()));
    }

    #[test]
    fn test_parse_open_graph_fallback() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Page Title</title>
                <meta name="description" content="Meta description">
            </head>
            <body></body>
            </html>
        "#;

        let data = parse_open_graph(html, "https://example.com");
        assert_eq!(data.title, Some("Page Title".to_string()));
        assert_eq!(data.description, Some("Meta description".to_string()));
    }

    #[test]
    fn test_parse_open_graph_relative_image() {
        let html = r#"
            <html>
            <head>
                <meta property="og:image" content="/images/og.png">
            </head>
            </html>
        "#;

        let data = parse_open_graph(html, "https://example.com/page");
        assert_eq!(data.image, Some("https://example.com/images/og.png".to_string()));
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("this is a longer string", 10), "this is a ...");
    }
}
