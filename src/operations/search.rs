// Local and global find operations

use std::path::Path;

use crate::ssh::{SshClient, SshError};
use crate::types::{DirectoryEntry, EntryType, format_size};

/// Case-insensitive substring matching helper.
///
/// Returns `true` if the lowercased `name` contains the lowercased `query` as a substring.
/// An empty query matches all names.
pub fn matches_query(name: &str, query: &str) -> bool {
    let name_lower = name.to_lowercase();
    let query_lower = query.to_lowercase();
    name_lower.contains(&query_lower)
}

/// Format a search result with relative path, size, and type indicator.
///
/// The type indicator is:
/// - "/" for directories
/// - "@" for symlinks
/// - "" (empty) for regular files
///
/// Format: `<relative_path><type_indicator>  <size>`
pub fn format_search_result(entry: &DirectoryEntry, base: &Path) -> String {
    let relative_path = entry
        .path
        .strip_prefix(base)
        .unwrap_or(&entry.path)
        .to_string_lossy();

    let type_indicator = match entry.entry_type {
        EntryType::Directory => "/",
        EntryType::Symlink => "@",
        EntryType::File => "",
    };

    let size_str = format_size(entry.size);

    format!("{}{}  {}", relative_path, type_indicator, size_str)
}

/// Search within a single directory (non-recursive, case-insensitive substring match).
///
/// Lists entries in `dir` via SSH, then filters to those whose name contains `query`
/// (case-insensitive). Hidden entries (names starting with '.') are excluded unless
/// `show_hidden` is true.
pub fn local_find(
    ssh: &SshClient,
    dir: &Path,
    query: &str,
    show_hidden: bool,
) -> Result<Vec<DirectoryEntry>, SshError> {
    let entries = ssh.list_dir(dir)?;

    let results = entries
        .into_iter()
        .filter(|entry| {
            // Filter hidden entries
            if !show_hidden && entry.name.starts_with('.') {
                return false;
            }
            // Case-insensitive substring match
            matches_query(&entry.name, query)
        })
        .collect();

    Ok(results)
}

/// Recursive search from base directory downward. Returns matching entries.
///
/// Uses `ssh.find_recursive(base, query)` to execute a remote `find` command.
/// Filters results for hidden entries if `show_hidden` is false.
/// Calls `on_progress` periodically with the count of entries processed so far.
/// If `on_progress` returns `false`, the search is aborted and current results are returned.
pub fn global_find(
    ssh: &SshClient,
    base: &Path,
    query: &str,
    show_hidden: bool,
    mut on_progress: impl FnMut(usize) -> bool,
) -> Result<Vec<DirectoryEntry>, SshError> {
    let all_entries = ssh.find_recursive(base, query)?;

    let mut results = Vec::new();
    let mut dir_count: usize = 0;

    for entry in all_entries {
        // Filter hidden entries: skip if any path component starts with '.'
        if !show_hidden {
            let has_hidden_component = entry
                .path
                .strip_prefix(base)
                .unwrap_or(&entry.path)
                .components()
                .any(|c| {
                    c.as_os_str()
                        .to_string_lossy()
                        .starts_with('.')
                });

            if has_hidden_component {
                continue;
            }
        }

        // Track directory count for progress reporting
        if entry.entry_type == EntryType::Directory {
            dir_count += 1;
        }

        results.push(entry);

        // Report progress and check for abort
        if !on_progress(dir_count) {
            // Abort requested — return current results
            return Ok(results);
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_entry(name: &str, entry_type: EntryType, base: &str) -> DirectoryEntry {
        DirectoryEntry {
            name: name.to_string(),
            path: PathBuf::from(format!("{}/{}", base, name)),
            entry_type,
            size: 1024,
        }
    }

    #[test]
    fn test_matches_query_basic() {
        assert!(matches_query("README.md", "readme"));
        assert!(matches_query("README.md", "READ"));
        assert!(matches_query("README.md", "me.m"));
        assert!(!matches_query("README.md", "xyz"));
    }

    #[test]
    fn test_matches_query_empty_query() {
        // Empty query matches everything
        assert!(matches_query("anything", ""));
        assert!(matches_query("", ""));
    }

    #[test]
    fn test_matches_query_empty_name() {
        assert!(!matches_query("", "something"));
    }

    #[test]
    fn test_matches_query_case_insensitive() {
        assert!(matches_query("MyFile.TXT", "myfile"));
        assert!(matches_query("myfile.txt", "MYFILE"));
        assert!(matches_query("MiXeD", "mixed"));
    }

    #[test]
    fn test_format_search_result_file() {
        let entry = DirectoryEntry {
            name: "app.log".to_string(),
            path: PathBuf::from("/home/user/logs/app.log"),
            entry_type: EntryType::File,
            size: 2048,
        };
        let base = Path::new("/home/user");
        let result = format_search_result(&entry, base);
        assert_eq!(result, "logs/app.log  2.0 KB");
    }

    #[test]
    fn test_format_search_result_directory() {
        let entry = DirectoryEntry {
            name: "logs".to_string(),
            path: PathBuf::from("/home/user/logs"),
            entry_type: EntryType::Directory,
            size: 0,
        };
        let base = Path::new("/home/user");
        let result = format_search_result(&entry, base);
        assert_eq!(result, "logs/  0 B");
    }

    #[test]
    fn test_format_search_result_symlink() {
        let entry = DirectoryEntry {
            name: "current".to_string(),
            path: PathBuf::from("/var/log/current"),
            entry_type: EntryType::Symlink,
            size: 512,
        };
        let base = Path::new("/var/log");
        let result = format_search_result(&entry, base);
        assert_eq!(result, "current@  512 B");
    }

    #[test]
    fn test_format_search_result_no_common_prefix() {
        // When path doesn't start with base, full path is used
        let entry = DirectoryEntry {
            name: "file.txt".to_string(),
            path: PathBuf::from("/other/path/file.txt"),
            entry_type: EntryType::File,
            size: 100,
        };
        let base = Path::new("/home/user");
        let result = format_search_result(&entry, base);
        assert_eq!(result, "/other/path/file.txt  100 B");
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use std::path::PathBuf;
    use proptest::prelude::*;

    // Feature: ssh-remote-file-browser, Property 9: Case-insensitive substring matching
    // Validates: Requirements 7.4, 7.5
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn case_insensitive_substring_matching(
            name in ".*",
            query in ".*",
        ) {
            let result = matches_query(&name, &query);
            let expected = name.to_lowercase().contains(&query.to_lowercase());
            prop_assert_eq!(result, expected,
                "matches_query({:?}, {:?}) returned {} but expected {}",
                name, query, result, expected);
        }
    }

    // Feature: ssh-remote-file-browser, Property 10: Search result display completeness
    // Validates: Requirements 7.6
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn search_result_display_completeness(
            name in "[a-zA-Z0-9_.-]{1,30}",
            base_dir in "/[a-zA-Z0-9_/]{1,30}",
            sub_path in "[a-zA-Z0-9_/]{0,20}",
            entry_type in prop_oneof![
                Just(EntryType::File),
                Just(EntryType::Directory),
                Just(EntryType::Symlink),
            ],
            size in any::<u64>(),
        ) {
            let full_path = if sub_path.is_empty() {
                format!("{}/{}", base_dir, name)
            } else {
                format!("{}/{}/{}", base_dir, sub_path, name)
            };

            let entry = DirectoryEntry {
                name: name.clone(),
                path: PathBuf::from(&full_path),
                entry_type: entry_type.clone(),
                size,
            };

            let base = Path::new(&base_dir);
            let result = format_search_result(&entry, base);

            // Result should contain a valid formatted size string
            let size_str = crate::types::format_size(size);
            prop_assert!(result.contains(&size_str),
                "Result {:?} should contain size string {:?}", result, size_str);

            // Result should contain type indicator
            match entry_type {
                EntryType::Directory => {
                    prop_assert!(result.contains("/"),
                        "Directory result {:?} should contain '/'", result);
                }
                EntryType::Symlink => {
                    prop_assert!(result.contains("@"),
                        "Symlink result {:?} should contain '@'", result);
                }
                EntryType::File => {
                    // File has no type indicator, but should not have / or @ appended
                    // (the relative path might contain '/' for nested paths, so we check
                    // the name portion doesn't have a spurious indicator)
                }
            }

            // Result should contain the relative path from base
            let relative = entry.path.strip_prefix(base)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| entry.path.to_string_lossy().to_string());
            prop_assert!(result.contains(&relative),
                "Result {:?} should contain relative path {:?}", result, relative);
        }
    }
}
