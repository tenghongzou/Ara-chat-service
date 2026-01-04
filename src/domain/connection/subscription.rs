//! Connection subscription management for conversation multiplexing
//!
//! This module provides per-connection conversation subscriptions,
//! allowing clients to only receive messages for specific conversations.

use std::sync::atomic::{AtomicU8, Ordering};

use dashmap::DashSet;
use uuid::Uuid;

/// Subscription mode for a connection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SubscriptionMode {
    /// Legacy mode: receive ALL messages (for backward compatibility)
    #[default]
    Legacy = 0,
    /// Explicit mode: only receive messages for subscribed conversations
    Explicit = 1,
}

impl From<u8> for SubscriptionMode {
    fn from(value: u8) -> Self {
        match value {
            1 => SubscriptionMode::Explicit,
            _ => SubscriptionMode::Legacy,
        }
    }
}

/// Result of a subscription operation
#[derive(Debug, Clone, Default)]
pub struct SubscriptionResult {
    /// Conversations that were newly subscribed
    pub subscribed: Vec<Uuid>,
    /// Conversations that were unsubscribed
    pub unsubscribed: Vec<Uuid>,
}

impl SubscriptionResult {
    /// Check if any changes were made
    pub fn has_changes(&self) -> bool {
        !self.subscribed.is_empty() || !self.unsubscribed.is_empty()
    }
}

/// Manages conversation subscriptions for a single connection
#[derive(Debug)]
pub struct ConnectionSubscriptions {
    /// Set of subscribed conversation IDs
    subscriptions: DashSet<Uuid>,
    /// Subscription mode (0=Legacy, 1=Explicit)
    mode: AtomicU8,
    /// Maximum subscriptions per connection
    max_subscriptions: usize,
}

impl ConnectionSubscriptions {
    /// Create a new subscription manager with a maximum subscription limit
    pub fn new(max_subscriptions: usize) -> Self {
        Self {
            subscriptions: DashSet::new(),
            mode: AtomicU8::new(SubscriptionMode::Legacy as u8),
            max_subscriptions,
        }
    }

    /// Subscribe to one or more conversations
    ///
    /// Automatically transitions to Explicit mode on first subscription.
    /// Returns the list of conversations that were actually added.
    pub fn subscribe(&self, conversation_ids: &[Uuid]) -> SubscriptionResult {
        // Transition to Explicit mode on first subscription
        self.mode.store(SubscriptionMode::Explicit as u8, Ordering::Relaxed);

        let mut added = Vec::new();
        for &id in conversation_ids {
            // Check limit
            if self.subscriptions.len() >= self.max_subscriptions {
                break;
            }
            // Insert returns true if the value was newly inserted
            if self.subscriptions.insert(id) {
                added.push(id);
            }
        }

        SubscriptionResult {
            subscribed: added,
            unsubscribed: vec![],
        }
    }

    /// Unsubscribe from one or more conversations
    ///
    /// Returns the list of conversations that were actually removed.
    pub fn unsubscribe(&self, conversation_ids: &[Uuid]) -> SubscriptionResult {
        let mut removed = Vec::new();
        for &id in conversation_ids {
            // Remove returns Some if the value was present
            if self.subscriptions.remove(&id).is_some() {
                removed.push(id);
            }
        }

        SubscriptionResult {
            subscribed: vec![],
            unsubscribed: removed,
        }
    }

    /// Check if a specific conversation is subscribed
    pub fn is_subscribed(&self, conversation_id: &Uuid) -> bool {
        self.subscriptions.contains(conversation_id)
    }

    /// Get the current subscription mode
    pub fn mode(&self) -> SubscriptionMode {
        self.mode.load(Ordering::Relaxed).into()
    }

    /// Check if in legacy mode (receive all messages)
    pub fn is_legacy_mode(&self) -> bool {
        self.mode() == SubscriptionMode::Legacy
    }

    /// Check if in explicit mode (only subscribed conversations)
    pub fn is_explicit_mode(&self) -> bool {
        self.mode() == SubscriptionMode::Explicit
    }

    /// Determine if a message for a conversation should be delivered to this connection
    ///
    /// System messages bypass the subscription filter.
    pub fn should_receive(&self, conversation_id: &Uuid, is_system_message: bool) -> bool {
        // System messages always delivered
        if is_system_message {
            return true;
        }

        // Legacy mode: receive everything
        if self.is_legacy_mode() {
            return true;
        }

        // Explicit mode: check subscription
        self.is_subscribed(conversation_id)
    }

    /// Get the number of subscriptions
    pub fn count(&self) -> usize {
        self.subscriptions.len()
    }

    /// Get the maximum subscription limit
    pub fn max(&self) -> usize {
        self.max_subscriptions
    }

    /// Get all subscribed conversation IDs
    pub fn subscribed_conversations(&self) -> Vec<Uuid> {
        self.subscriptions.iter().map(|r| *r).collect()
    }

    /// Clear all subscriptions (for cleanup)
    pub fn clear(&self) {
        self.subscriptions.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_starts_in_legacy_mode() {
        let subs = ConnectionSubscriptions::new(100);
        assert!(subs.is_legacy_mode());
        assert!(!subs.is_explicit_mode());
        assert_eq!(subs.count(), 0);
    }

    #[test]
    fn test_subscribe_transitions_to_explicit_mode() {
        let subs = ConnectionSubscriptions::new(100);
        let conv_id = Uuid::new_v4();

        assert!(subs.is_legacy_mode());
        subs.subscribe(&[conv_id]);
        assert!(subs.is_explicit_mode());
    }

    #[test]
    fn test_subscribe_adds_conversations() {
        let subs = ConnectionSubscriptions::new(100);
        let conv1 = Uuid::new_v4();
        let conv2 = Uuid::new_v4();

        let result = subs.subscribe(&[conv1, conv2]);

        assert_eq!(result.subscribed.len(), 2);
        assert!(result.subscribed.contains(&conv1));
        assert!(result.subscribed.contains(&conv2));
        assert!(subs.is_subscribed(&conv1));
        assert!(subs.is_subscribed(&conv2));
        assert_eq!(subs.count(), 2);
    }

    #[test]
    fn test_subscribe_ignores_duplicates() {
        let subs = ConnectionSubscriptions::new(100);
        let conv_id = Uuid::new_v4();

        let result1 = subs.subscribe(&[conv_id]);
        let result2 = subs.subscribe(&[conv_id]);

        assert_eq!(result1.subscribed.len(), 1);
        assert_eq!(result2.subscribed.len(), 0);
        assert_eq!(subs.count(), 1);
    }

    #[test]
    fn test_subscribe_respects_limit() {
        let subs = ConnectionSubscriptions::new(2);
        let conv1 = Uuid::new_v4();
        let conv2 = Uuid::new_v4();
        let conv3 = Uuid::new_v4();

        let result = subs.subscribe(&[conv1, conv2, conv3]);

        assert_eq!(result.subscribed.len(), 2);
        assert_eq!(subs.count(), 2);
    }

    #[test]
    fn test_unsubscribe_removes_conversations() {
        let subs = ConnectionSubscriptions::new(100);
        let conv1 = Uuid::new_v4();
        let conv2 = Uuid::new_v4();

        subs.subscribe(&[conv1, conv2]);
        let result = subs.unsubscribe(&[conv1]);

        assert_eq!(result.unsubscribed.len(), 1);
        assert!(result.unsubscribed.contains(&conv1));
        assert!(!subs.is_subscribed(&conv1));
        assert!(subs.is_subscribed(&conv2));
        assert_eq!(subs.count(), 1);
    }

    #[test]
    fn test_unsubscribe_nonexistent_is_noop() {
        let subs = ConnectionSubscriptions::new(100);
        let conv_id = Uuid::new_v4();

        let result = subs.unsubscribe(&[conv_id]);

        assert!(result.unsubscribed.is_empty());
    }

    #[test]
    fn test_should_receive_legacy_mode() {
        let subs = ConnectionSubscriptions::new(100);
        let conv_id = Uuid::new_v4();

        // Legacy mode receives all messages
        assert!(subs.should_receive(&conv_id, false));
        assert!(subs.should_receive(&conv_id, true));
    }

    #[test]
    fn test_should_receive_explicit_mode_subscribed() {
        let subs = ConnectionSubscriptions::new(100);
        let conv_id = Uuid::new_v4();

        subs.subscribe(&[conv_id]);

        // Subscribed conversation should receive
        assert!(subs.should_receive(&conv_id, false));
    }

    #[test]
    fn test_should_receive_explicit_mode_not_subscribed() {
        let subs = ConnectionSubscriptions::new(100);
        let conv1 = Uuid::new_v4();
        let conv2 = Uuid::new_v4();

        subs.subscribe(&[conv1]); // Only subscribe to conv1

        // Not subscribed should not receive
        assert!(!subs.should_receive(&conv2, false));
    }

    #[test]
    fn test_should_receive_system_message_bypasses_filter() {
        let subs = ConnectionSubscriptions::new(100);
        let conv1 = Uuid::new_v4();
        let conv2 = Uuid::new_v4();

        subs.subscribe(&[conv1]); // Only subscribe to conv1

        // System messages always delivered
        assert!(subs.should_receive(&conv2, true));
    }

    #[test]
    fn test_subscribed_conversations() {
        let subs = ConnectionSubscriptions::new(100);
        let conv1 = Uuid::new_v4();
        let conv2 = Uuid::new_v4();

        subs.subscribe(&[conv1, conv2]);

        let all = subs.subscribed_conversations();
        assert_eq!(all.len(), 2);
        assert!(all.contains(&conv1));
        assert!(all.contains(&conv2));
    }

    #[test]
    fn test_clear() {
        let subs = ConnectionSubscriptions::new(100);
        let conv_id = Uuid::new_v4();

        subs.subscribe(&[conv_id]);
        assert_eq!(subs.count(), 1);

        subs.clear();
        assert_eq!(subs.count(), 0);
    }

    #[test]
    fn test_subscription_result_has_changes() {
        let empty = SubscriptionResult::default();
        assert!(!empty.has_changes());

        let with_subscribe = SubscriptionResult {
            subscribed: vec![Uuid::new_v4()],
            unsubscribed: vec![],
        };
        assert!(with_subscribe.has_changes());

        let with_unsubscribe = SubscriptionResult {
            subscribed: vec![],
            unsubscribed: vec![Uuid::new_v4()],
        };
        assert!(with_unsubscribe.has_changes());
    }
}
