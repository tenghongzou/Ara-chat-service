//! Mention parser - extracts @user mentions from message content

use uuid::Uuid;

/// Parses @mentions from message content
pub struct MentionParser;

impl MentionParser {
    /// Extract mentioned user IDs from message content
    /// Expects format: @[uuid] or @username
    pub fn parse(content: &str) -> Vec<MentionMatch> {
        let mut mentions = Vec::new();

        // Pattern 1: @[uuid] format (explicit user ID)
        for caps in regex_lite::Regex::new(r"@\[([0-9a-f-]{36})\]")
            .unwrap()
            .captures_iter(content)
        {
            if let Some(uuid_str) = caps.get(1) {
                if let Ok(user_id) = Uuid::parse_str(uuid_str.as_str()) {
                    mentions.push(MentionMatch {
                        user_id: Some(user_id),
                        username: None,
                        start: caps.get(0).unwrap().start(),
                        end: caps.get(0).unwrap().end(),
                    });
                }
            }
        }

        // Pattern 2: @username format (needs resolution)
        for caps in regex_lite::Regex::new(r"@(\w+)")
            .unwrap()
            .captures_iter(content)
        {
            if let Some(username) = caps.get(1) {
                // Skip if this is part of a UUID mention
                let start = caps.get(0).unwrap().start();
                if content.get(start.saturating_sub(1)..start) == Some("[") {
                    continue;
                }

                mentions.push(MentionMatch {
                    user_id: None,
                    username: Some(username.as_str().to_string()),
                    start,
                    end: caps.get(0).unwrap().end(),
                });
            }
        }

        mentions
    }

    /// Validate that all mentioned users are participants in the conversation
    pub fn validate_mentions(mentions: &[Uuid], participants: &[Uuid]) -> Vec<Uuid> {
        mentions
            .iter()
            .filter(|m| participants.contains(m))
            .copied()
            .collect()
    }
}

/// A parsed mention match
#[derive(Debug, Clone)]
pub struct MentionMatch {
    pub user_id: Option<Uuid>,
    pub username: Option<String>,
    pub start: usize,
    pub end: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_uuid_mention() {
        let user_id = Uuid::new_v4();
        let content = format!("Hello @[{}] how are you?", user_id);

        let mentions = MentionParser::parse(&content);
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].user_id, Some(user_id));
    }

    #[test]
    fn test_parse_username_mention() {
        let content = "Hello @john how are you?";

        let mentions = MentionParser::parse(content);
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].username, Some("john".to_string()));
    }

    #[test]
    fn test_parse_multiple_mentions() {
        let user_id = Uuid::new_v4();
        let content = format!("Hey @alice and @[{}] check this out", user_id);

        let mentions = MentionParser::parse(&content);
        assert_eq!(mentions.len(), 2);
    }
}
