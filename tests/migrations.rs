//! Database migration tests
//!
//! These tests verify that migration files are valid and properly structured.
//! Run with: cargo test --test migrations

use std::fs;
use std::path::Path;

/// Test that all migration files exist and are readable
#[test]
fn test_migrations_exist() {
    let migrations_dir = Path::new("migrations");
    assert!(
        migrations_dir.exists(),
        "Migrations directory does not exist"
    );

    let expected_migrations = [
        "001_conversations.sql",
        "002_messages.sql",
        "003_reactions.sql",
        "004_read_receipts.sql",
        "005_fulltext_search_index.sql",
        "006_attachments.sql",
        "007_message_threading.sql",
    ];

    for migration in &expected_migrations {
        let path = migrations_dir.join(migration);
        assert!(
            path.exists(),
            "Migration file {} does not exist",
            migration
        );
    }
}

/// Test that migrations are properly numbered and ordered
#[test]
fn test_migration_numbering() {
    let migrations_dir = Path::new("migrations");
    let mut entries: Vec<_> = fs::read_dir(migrations_dir)
        .expect("Failed to read migrations directory")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "sql"))
        .collect();

    entries.sort_by_key(|e| e.file_name());

    // Verify sequential numbering
    for (i, entry) in entries.iter().enumerate() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let expected_prefix = format!("{:03}", i + 1);
        assert!(
            name_str.starts_with(&expected_prefix),
            "Migration {} should start with {}",
            name_str,
            expected_prefix
        );
    }
}

/// Test that each migration contains valid SQL syntax markers
#[test]
fn test_migration_sql_syntax() {
    let migrations_dir = Path::new("migrations");
    let entries: Vec<_> = fs::read_dir(migrations_dir)
        .expect("Failed to read migrations directory")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "sql"))
        .collect();

    for entry in entries {
        let path = entry.path();
        let content = fs::read_to_string(&path).expect("Failed to read migration file");

        // Migration should not be empty
        assert!(
            !content.trim().is_empty(),
            "Migration {} is empty",
            path.display()
        );

        // Should contain at least one SQL statement (CREATE, ALTER, INSERT, etc.)
        let has_sql_statement = content.contains("CREATE")
            || content.contains("ALTER")
            || content.contains("INSERT")
            || content.contains("UPDATE")
            || content.contains("DROP")
            || content.contains("DELETE")
            || content.contains("INDEX");

        assert!(
            has_sql_statement,
            "Migration {} does not contain any SQL statements",
            path.display()
        );

        // Should not contain dangerous unqualified DROP DATABASE/SCHEMA
        assert!(
            !content.contains("DROP DATABASE")
                && !content.contains("DROP SCHEMA")
                && !content.contains("TRUNCATE"),
            "Migration {} contains dangerous statements",
            path.display()
        );
    }
}

/// Test conversations migration structure
#[test]
fn test_001_conversations_migration() {
    let content = fs::read_to_string("migrations/001_conversations.sql")
        .expect("Failed to read migration");

    // Should create conversations table
    assert!(
        content.contains("CREATE TABLE") && content.contains("conversations"),
        "Should create conversations table"
    );

    // Should create participants table
    assert!(
        content.contains("participants"),
        "Should handle conversation participants"
    );

    // Should have IF NOT EXISTS for idempotency
    assert!(
        content.contains("IF NOT EXISTS"),
        "Should be idempotent with IF NOT EXISTS"
    );
}

/// Test messages migration structure
#[test]
fn test_002_messages_migration() {
    let content =
        fs::read_to_string("migrations/002_messages.sql").expect("Failed to read migration");

    // Should create messages table
    assert!(
        content.contains("CREATE TABLE") && content.contains("messages"),
        "Should create messages table"
    );

    // Should have foreign key to conversations
    assert!(
        content.contains("conversation_id"),
        "Should reference conversation_id"
    );

    // Should have sender_id
    assert!(content.contains("sender_id"), "Should have sender_id");

    // Should have content field
    assert!(content.contains("content"), "Should have content field");

    // Should be idempotent
    assert!(
        content.contains("IF NOT EXISTS"),
        "Should be idempotent with IF NOT EXISTS"
    );
}

/// Test reactions migration structure
#[test]
fn test_003_reactions_migration() {
    let content =
        fs::read_to_string("migrations/003_reactions.sql").expect("Failed to read migration");

    // Should create reactions table
    assert!(
        content.contains("CREATE TABLE") && content.contains("reactions"),
        "Should create reactions table"
    );

    // Should reference message_id
    assert!(
        content.contains("message_id"),
        "Should reference message_id"
    );

    // Should have emoji field
    assert!(content.contains("emoji"), "Should have emoji field");
}

/// Test read receipts migration structure
#[test]
fn test_004_read_receipts_migration() {
    let content =
        fs::read_to_string("migrations/004_read_receipts.sql").expect("Failed to read migration");

    // Should create read_receipts table
    assert!(
        content.contains("CREATE TABLE") && content.contains("read_receipts"),
        "Should create read_receipts table"
    );

    // Should reference conversation and user
    assert!(
        content.contains("conversation_id") && content.contains("user_id"),
        "Should reference conversation_id and user_id"
    );
}

/// Test fulltext search index migration
#[test]
fn test_005_fulltext_index_migration() {
    let content = fs::read_to_string("migrations/005_fulltext_search_index.sql")
        .expect("Failed to read migration");

    // Should create GIN index
    assert!(
        content.contains("GIN") || content.contains("gin"),
        "Should create GIN index for full-text search"
    );

    // Should create index on messages
    assert!(
        content.contains("messages"),
        "Should create index on messages table"
    );

    // Should use to_tsvector
    assert!(
        content.contains("to_tsvector"),
        "Should use to_tsvector for full-text search"
    );
}

/// Test attachments migration structure
#[test]
fn test_006_attachments_migration() {
    let content = fs::read_to_string("migrations/006_attachments.sql")
        .expect("Failed to read migration");

    // Should create attachments table
    assert!(
        content.contains("CREATE TABLE") && content.contains("attachments"),
        "Should create attachments table"
    );

    // Should reference conversation_id
    assert!(
        content.contains("conversation_id"),
        "Should reference conversation_id"
    );

    // Should have file metadata fields
    assert!(content.contains("file_name"), "Should have file_name");
    assert!(content.contains("file_size"), "Should have file_size");
    assert!(content.contains("mime_type"), "Should have mime_type");
    assert!(content.contains("content_hash"), "Should have content_hash");

    // Should have storage fields
    assert!(content.contains("storage_backend"), "Should have storage_backend");
    assert!(content.contains("storage_path"), "Should have storage_path");

    // Should have thumbnail support
    assert!(content.contains("thumbnail_path"), "Should have thumbnail_path");

    // Should be idempotent
    assert!(
        content.contains("IF NOT EXISTS"),
        "Should be idempotent with IF NOT EXISTS"
    );

    // Should have file size constraint
    assert!(
        content.contains("52428800"),
        "Should have 50MB file size limit"
    );
}

/// Test message threading migration structure
#[test]
fn test_007_message_threading_migration() {
    let content = fs::read_to_string("migrations/007_message_threading.sql")
        .expect("Failed to read migration");

    // Should create index for reply lookups
    assert!(
        content.contains("idx_messages_reply_to"),
        "Should create reply_to index"
    );

    // Should create index for thread counting
    assert!(
        content.contains("idx_messages_thread_count"),
        "Should create thread count index"
    );

    // Should use partial index (WHERE reply_to_id IS NOT NULL)
    assert!(
        content.contains("WHERE reply_to_id IS NOT NULL"),
        "Should use partial index for efficiency"
    );

    // Should be idempotent
    assert!(
        content.contains("IF NOT EXISTS"),
        "Should be idempotent with IF NOT EXISTS"
    );

    // Should have descriptive comment
    assert!(
        content.contains("thread") || content.contains("reply"),
        "Should have descriptive comments about threading"
    );
}

/// Test that migrations are consistently formatted
#[test]
fn test_migrations_have_consistent_style() {
    let migrations_dir = Path::new("migrations");

    for entry in fs::read_dir(migrations_dir).expect("Failed to read migrations directory") {
        let entry = entry.expect("Failed to read directory entry");
        if entry.path().extension().map_or(false, |ext| ext == "sql") {
            let content =
                fs::read_to_string(entry.path()).expect("Failed to read migration file");
            let file_name = entry.file_name().to_string_lossy().to_string();

            // Each migration should have a comment header explaining what it does
            let has_comment = content.contains("--") || content.contains("/*");
            assert!(
                has_comment,
                "Migration {} should have descriptive comments",
                file_name
            );

            // Check that there's at least one statement that ends with semicolon
            let has_semicolon = content.contains(';');
            assert!(
                has_semicolon,
                "Migration {} should contain SQL statements ending with semicolons",
                file_name
            );
        }
    }
}
