// Shared types: DirectoryEntry, StatusMessage, formatting utilities

use std::path::PathBuf;
use std::time::Instant;

/// Represents a file system entry type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryType {
    File,
    Directory,
    Symlink,
}

/// Represents a single entry in a remote directory listing.
#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    pub name: String,
    pub path: PathBuf,
    pub entry_type: EntryType,
    pub size: u64,
}

/// Severity level for status messages displayed in the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusLevel {
    Info,
    Success,
    Error,
}

/// A status message shown in the footer bar of the TUI.
#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub text: String,
    pub level: StatusLevel,
    pub created_at: Instant,
}

/// Format a byte count into a human-readable string.
///
/// Uses the following units:
/// - B for values < 1024
/// - KB for values < 1 MB (1,048,576)
/// - MB for values < 1 GB (1,073,741,824)
/// - GB for values >= 1 GB
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;

    if bytes < KB {
        format!("{} B", bytes)
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else if bytes < GB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    }
}

/// Truncate a name to `max_len` characters, appending "..." if it exceeds the limit.
///
/// If `name` fits within `max_len`, it is returned as-is.
/// If `name` exceeds `max_len`, the first `max_len` characters are kept and "..." is appended.
pub fn truncate_name(name: &str, max_len: usize) -> String {
    if name.chars().count() <= max_len {
        name.to_string()
    } else {
        let truncated: String = name.chars().take(max_len).collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn test_format_size_kilobytes() {
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1048575), "1024.0 KB");
    }

    #[test]
    fn test_format_size_megabytes() {
        assert_eq!(format_size(1048576), "1.0 MB");
        assert_eq!(format_size(1073741823), "1024.0 MB");
    }

    #[test]
    fn test_format_size_gigabytes() {
        assert_eq!(format_size(1073741824), "1.0 GB");
        assert_eq!(format_size(2147483648), "2.0 GB");
    }

    #[test]
    fn test_truncate_name_short() {
        assert_eq!(truncate_name("hello.txt", 60), "hello.txt");
        assert_eq!(truncate_name("", 60), "");
    }

    #[test]
    fn test_truncate_name_exact() {
        let name = "a".repeat(60);
        assert_eq!(truncate_name(&name, 60), name);
    }

    #[test]
    fn test_truncate_name_overflow() {
        let name = "a".repeat(61);
        let expected = format!("{}...", "a".repeat(60));
        assert_eq!(truncate_name(&name, 60), expected);
    }

    #[test]
    fn test_truncate_name_unicode() {
        // Unicode characters should be counted by char, not byte
        let name = "日本語テスト"; // 6 chars
        assert_eq!(truncate_name(name, 6), "日本語テスト");
        assert_eq!(truncate_name(name, 3), "日本語...");
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    // Feature: ssh-remote-file-browser, Property 3: Entry display formatting
    // Validates: Requirements 2.2
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn truncate_name_length_invariant(
            name in ".{0,200}",
            max_len in 1usize..=100,
        ) {
            let result = truncate_name(&name, max_len);
            let name_char_count = name.chars().count();
            let result_char_count = result.chars().count();

            if name_char_count <= max_len {
                // When name fits, result equals original
                prop_assert_eq!(&result, &name);
            } else {
                // When truncated, result ends with "..." and has exactly max_len + 3 chars
                prop_assert!(result.ends_with("..."),
                    "Truncated result should end with '...', got: {:?}", result);
                prop_assert_eq!(result_char_count, max_len + 3,
                    "Truncated result should have max_len + 3 chars, got {} (max_len={})",
                    result_char_count, max_len);
            }

            // Universal: result length is always <= max_len + 3
            prop_assert!(result_char_count <= max_len + 3,
                "Result char count {} exceeds max_len + 3 = {}", result_char_count, max_len + 3);
        }

        #[test]
        fn format_size_non_empty_and_pattern(bytes: u64) {
            let result = format_size(bytes);

            // Result is non-empty
            prop_assert!(!result.is_empty(), "format_size should return non-empty string");

            // Result matches pattern: <number> <unit> where unit is B, KB, MB, or GB
            let parts: Vec<&str> = result.rsplitn(2, ' ').collect();
            prop_assert_eq!(parts.len(), 2,
                "format_size result should have format '<number> <unit>', got: {:?}", result);

            let unit = parts[0];
            let number_part = parts[1];

            // Unit must be one of B, KB, MB, GB
            prop_assert!(
                unit == "B" || unit == "KB" || unit == "MB" || unit == "GB",
                "Unit should be B, KB, MB, or GB, got: {:?}", unit
            );

            // Number part should be parseable (either integer for B or float for KB/MB/GB)
            if unit == "B" {
                prop_assert!(number_part.parse::<u64>().is_ok(),
                    "Byte value should be a valid integer, got: {:?}", number_part);
            } else {
                prop_assert!(number_part.parse::<f64>().is_ok(),
                    "Size value should be a valid number, got: {:?}", number_part);
            }
        }
    }
}
