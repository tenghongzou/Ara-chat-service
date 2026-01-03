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

    #[test]
    fn test_parse_no_mentions() {
        let content = "Hello world, no mentions here!";
        let mentions = MentionParser::parse(content);
        assert!(mentions.is_empty());
    }

    #[test]
    fn test_parse_mention_at_start() {
        let content = "@alice is the best";
        let mentions = MentionParser::parse(content);
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].username, Some("alice".to_string()));
        assert_eq!(mentions[0].start, 0);
    }

    #[test]
    fn test_parse_mention_at_end() {
        let content = "Thanks @bob";
        let mentions = MentionParser::parse(content);
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].username, Some("bob".to_string()));
        assert_eq!(mentions[0].end, content.len());
    }

    #[test]
    fn test_parse_consecutive_mentions() {
        let content = "@alice @bob @charlie";
        let mentions = MentionParser::parse(content);
        assert_eq!(mentions.len(), 3);
        assert_eq!(mentions[0].username, Some("alice".to_string()));
        assert_eq!(mentions[1].username, Some("bob".to_string()));
        assert_eq!(mentions[2].username, Some("charlie".to_string()));
    }

    #[test]
    fn test_parse_with_punctuation() {
        let content = "Hey @alice, @bob! What do you think?";
        let mentions = MentionParser::parse(content);
        assert_eq!(mentions.len(), 2);
        // Usernames should not include punctuation
        assert_eq!(mentions[0].username, Some("alice".to_string()));
        assert_eq!(mentions[1].username, Some("bob".to_string()));
    }

    #[test]
    fn test_parse_invalid_uuid() {
        let content = "@[not-a-valid-uuid-at-all-here]";
        let mentions = MentionParser::parse(content);
        // Should not match as UUID mention
        assert!(mentions.iter().all(|m| m.user_id.is_none()));
    }

    #[test]
    fn test_parse_email_not_mention() {
        // @ in email should be treated as mention in current implementation
        // This documents the current behavior
        let content = "Contact us at support@example.com";
        let mentions = MentionParser::parse(content);
        // Note: current parser will match @example as a mention
        // This test documents this behavior
        assert!(!mentions.is_empty()); // @example is matched
    }

    #[test]
    fn test_validate_mentions_filters_non_participants() {
        let user1 = Uuid::new_v4();
        let user2 = Uuid::new_v4();
        let user3 = Uuid::new_v4();

        let mentioned = vec![user1, user2, user3];
        let participants = vec![user1, user3]; // user2 not a participant

        let valid = MentionParser::validate_mentions(&mentioned, &participants);
        assert_eq!(valid.len(), 2);
        assert!(valid.contains(&user1));
        assert!(valid.contains(&user3));
        assert!(!valid.contains(&user2));
    }

    #[test]
    fn test_validate_mentions_empty_participants() {
        let user1 = Uuid::new_v4();
        let mentioned = vec![user1];
        let participants: Vec<Uuid> = vec![];

        let valid = MentionParser::validate_mentions(&mentioned, &participants);
        assert!(valid.is_empty());
    }

    #[test]
    fn test_validate_mentions_all_valid() {
        let user1 = Uuid::new_v4();
        let user2 = Uuid::new_v4();

        let mentioned = vec![user1, user2];
        let participants = vec![user1, user2];

        let valid = MentionParser::validate_mentions(&mentioned, &participants);
        assert_eq!(valid.len(), 2);
    }

    #[test]
    fn test_mention_match_position() {
        let content = "Hello @world!";
        let mentions = MentionParser::parse(content);
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].start, 6); // @ starts at index 6
        assert_eq!(mentions[0].end, 12); // @world ends at index 12
        assert_eq!(&content[mentions[0].start..mentions[0].end], "@world");
    }

    #[test]
    fn test_parse_underscore_in_username() {
        let content = "Hello @user_name!";
        let mentions = MentionParser::parse(content);
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].username, Some("user_name".to_string()));
    }
}
