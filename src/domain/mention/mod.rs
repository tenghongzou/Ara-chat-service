//! Mention domain - @user mentions in messages

mod parser;
mod notifier;

pub use parser::MentionParser;
pub use notifier::MentionNotifier;
