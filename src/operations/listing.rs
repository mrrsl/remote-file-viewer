// Directory listing logic

use std::path::{Path, PathBuf};

use crate::ssh::{SshClient, SshError};
use crate::types::{DirectoryEntry, EntryType};

/// Compute the parent path. If the path is root ("/"), return root itself.
pub fn parent_path(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if parent == Path::new("") => PathBuf::from("/"),
        Some(parent) => parent.to_path_buf(),
        None => PathBuf::from("/"),
    }
}

/// Resolve a relative path against a working directory.
///
/// E.g., `resolve_relative("logs/app.log", Path::new("/home/user"))` → `/home/user/logs/app.log`
pub fn resolve_relative(relative: &str, working_dir: &Path) -> PathBuf {
    working_dir.join(relative)
}

/// List and sort directory entries. Filters hidden entries if `show_hidden` is false.
///
/// Calls `ssh.list_dir(path)`, removes entries whose name starts with '.'
/// when `show_hidden` is false, and sorts the result using `sort_entries`.
/// If `use_sudo` is true and SFTP returns PermissionDenied, retries with sudo.
pub fn list_directory(
    ssh: &SshClient,
    path: &Path,
    show_hidden: bool,
    use_sudo: bool,
) -> Result<Vec<DirectoryEntry>, SshError> {
    let mut entries = match ssh.list_dir(path) {
        Ok(e) => e,
        Err(SshError::PermissionDenied(_)) if use_sudo => {
            ssh.sudo_list_dir(path)?
        }
        Err(e) => return Err(e),
    };

    if !show_hidden {
        entries.retain(|entry| !entry.name.starts_with('.'));
    }

    sort_entries(&mut entries);
    Ok(entries)
}

/// Sort entries: directories first, then alphabetical case-insensitive within each group.
///
/// Ordering rules:
/// 1. Directories come before Files and Symlinks
/// 2. Within the same type group, entries are sorted by case-insensitive name comparison
pub fn sort_entries(entries: &mut Vec<DirectoryEntry>) {
    entries.sort_by(|a, b| {
        let a_is_dir = a.entry_type == EntryType::Directory;
        let b_is_dir = b.entry_type == EntryType::Directory;

        match (a_is_dir, b_is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_entry(name: &str, entry_type: EntryType) -> DirectoryEntry {
        DirectoryEntry {
            name: name.to_string(),
            path: PathBuf::from(format!("/test/{}", name)),
            entry_type,
            size: 0,
        }
    }

    #[test]
    fn test_sort_entries_directories_before_files() {
        let mut entries = vec![
            make_entry("beta.txt", EntryType::File),
            make_entry("alpha_dir", EntryType::Directory),
            make_entry("gamma.log", EntryType::File),
            make_entry("delta_dir", EntryType::Directory),
        ];

        sort_entries(&mut entries);

        assert_eq!(entries[0].name, "alpha_dir");
        assert_eq!(entries[1].name, "delta_dir");
        assert_eq!(entries[2].name, "beta.txt");
        assert_eq!(entries[3].name, "gamma.log");
    }

    #[test]
    fn test_sort_entries_case_insensitive() {
        let mut entries = vec![
            make_entry("Zebra", EntryType::File),
            make_entry("apple", EntryType::File),
            make_entry("Banana", EntryType::File),
        ];

        sort_entries(&mut entries);

        assert_eq!(entries[0].name, "apple");
        assert_eq!(entries[1].name, "Banana");
        assert_eq!(entries[2].name, "Zebra");
    }

    #[test]
    fn test_sort_entries_symlinks_grouped_with_files() {
        let mut entries = vec![
            make_entry("link_a", EntryType::Symlink),
            make_entry("dir_b", EntryType::Directory),
            make_entry("file_c", EntryType::File),
        ];

        sort_entries(&mut entries);

        // Directory first, then symlinks and files sorted together
        assert_eq!(entries[0].name, "dir_b");
        assert_eq!(entries[0].entry_type, EntryType::Directory);
        // file_c and link_a sorted alphabetically
        assert_eq!(entries[1].name, "file_c");
        assert_eq!(entries[2].name, "link_a");
    }

    #[test]
    fn test_sort_entries_empty() {
        let mut entries: Vec<DirectoryEntry> = vec![];
        sort_entries(&mut entries);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_sort_entries_single_entry() {
        let mut entries = vec![make_entry("only", EntryType::File)];
        sort_entries(&mut entries);
        assert_eq!(entries[0].name, "only");
    }

    // Tests for parent_path

    #[test]
    fn test_parent_path_root_returns_root() {
        let root = Path::new("/");
        assert_eq!(parent_path(root), PathBuf::from("/"));
    }

    #[test]
    fn test_parent_path_single_level() {
        let path = Path::new("/home");
        assert_eq!(parent_path(path), PathBuf::from("/"));
    }

    #[test]
    fn test_parent_path_nested() {
        let path = Path::new("/home/user/documents");
        assert_eq!(parent_path(path), PathBuf::from("/home/user"));
    }

    #[test]
    fn test_parent_path_deeply_nested() {
        let path = Path::new("/a/b/c/d/e");
        assert_eq!(parent_path(path), PathBuf::from("/a/b/c/d"));
    }

    // Tests for resolve_relative

    #[test]
    fn test_resolve_relative_simple_file() {
        let result = resolve_relative("app.log", Path::new("/home/user"));
        assert_eq!(result, PathBuf::from("/home/user/app.log"));
    }

    #[test]
    fn test_resolve_relative_nested_path() {
        let result = resolve_relative("logs/app.log", Path::new("/home/user"));
        assert_eq!(result, PathBuf::from("/home/user/logs/app.log"));
    }

    #[test]
    fn test_resolve_relative_against_root() {
        let result = resolve_relative("data/file.txt", Path::new("/"));
        assert_eq!(result, PathBuf::from("/data/file.txt"));
    }

    #[test]
    fn test_resolve_relative_deep_working_dir() {
        let result = resolve_relative("output.csv", Path::new("/var/log/kafka"));
        assert_eq!(result, PathBuf::from("/var/log/kafka/output.csv"));
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    use std::path::PathBuf;

    // Feature: ssh-remote-file-browser, Property 4: Entry sorting invariant
    // Validates: Requirements 2.3

    fn arb_entry_type() -> impl Strategy<Value = EntryType> {
        prop_oneof![
            Just(EntryType::File),
            Just(EntryType::Directory),
            Just(EntryType::Symlink),
        ]
    }

    fn arb_directory_entry() -> impl Strategy<Value = DirectoryEntry> {
        ("[a-zA-Z0-9_. ]{1,30}", arb_entry_type()).prop_map(|(name, entry_type)| DirectoryEntry {
            path: PathBuf::from(format!("/test/{}", name)),
            name,
            entry_type,
            size: 0,
        })
    }

    /// Generate a random absolute path with 0-20 components.
    /// Components are non-empty strings of alphanumeric characters.
    fn arb_absolute_path() -> impl Strategy<Value = PathBuf> {
        proptest::collection::vec("[a-zA-Z0-9_]{1,10}", 0..=20).prop_map(|components| {
            if components.is_empty() {
                PathBuf::from("/")
            } else {
                PathBuf::from(format!("/{}", components.join("/")))
            }
        })
    }

    /// Generate a random relative path (non-empty, no null bytes).
    fn arb_relative_path() -> impl Strategy<Value = String> {
        proptest::collection::vec("[a-zA-Z0-9_]{1,10}", 1..=5)
            .prop_map(|components| components.join("/"))
    }

    /// Generate a random absolute working directory.
    fn arb_working_dir() -> impl Strategy<Value = PathBuf> {
        proptest::collection::vec("[a-zA-Z0-9_]{1,10}", 1..=5).prop_map(|components| {
            PathBuf::from(format!("/{}", components.join("/")))
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn entry_sorting_invariant(
            entries in proptest::collection::vec(arb_directory_entry(), 0..=100),
        ) {
            let mut sorted = entries.clone();
            sort_entries(&mut sorted);

            // Find the boundary between directories and non-directories
            let first_non_dir = sorted.iter().position(|e| e.entry_type != EntryType::Directory);
            let last_dir = sorted.iter().rposition(|e| e.entry_type == EntryType::Directory);

            // All directories appear before any file/symlink
            if let (Some(last_d), Some(first_nd)) = (last_dir, first_non_dir) {
                prop_assert!(last_d < first_nd,
                    "All directories should appear before files/symlinks. \
                     Last directory at index {}, first non-directory at index {}",
                    last_d, first_nd);
            }

            // Within the directory group: entries are sorted by name.to_lowercase()
            let dir_group: Vec<&DirectoryEntry> = sorted.iter()
                .filter(|e| e.entry_type == EntryType::Directory)
                .collect();
            for window in dir_group.windows(2) {
                prop_assert!(
                    window[0].name.to_lowercase() <= window[1].name.to_lowercase(),
                    "Directories should be sorted case-insensitively: {:?} should come before {:?}",
                    window[0].name, window[1].name
                );
            }

            // Within the file/symlink group: entries are sorted by name.to_lowercase()
            let file_group: Vec<&DirectoryEntry> = sorted.iter()
                .filter(|e| e.entry_type != EntryType::Directory)
                .collect();
            for window in file_group.windows(2) {
                prop_assert!(
                    window[0].name.to_lowercase() <= window[1].name.to_lowercase(),
                    "Files/symlinks should be sorted case-insensitively: {:?} should come before {:?}",
                    window[0].name, window[1].name
                );
            }
        }

        // Feature: ssh-remote-file-browser, Property 6: Parent path navigation
        // Validates: Requirements 3.7, 3.8

        #[test]
        fn parent_path_navigation(path in arb_absolute_path()) {
            let result = parent_path(&path);

            if path == PathBuf::from("/") {
                // Parent of root is root
                prop_assert_eq!(result, PathBuf::from("/"),
                    "Parent of root '/' should be '/'");
            } else {
                // Parent of non-root removes last component
                let path_str = path.to_string_lossy().to_string();
                let components: Vec<&str> = path_str.split('/').filter(|s| !s.is_empty()).collect();
                prop_assert!(!components.is_empty(),
                    "Non-root path should have at least one component");

                // The parent should have one fewer component
                let expected_components: Vec<&str> = components[..components.len() - 1].to_vec();
                let expected = if expected_components.is_empty() {
                    PathBuf::from("/")
                } else {
                    PathBuf::from(format!("/{}", expected_components.join("/")))
                };

                prop_assert_eq!(result, expected,
                    "Parent of {:?} was incorrect", path);
            }
        }

        // Feature: ssh-remote-file-browser, Property 7: Relative path resolution
        // Validates: Requirements 5.3

        #[test]
        fn relative_path_resolution(
            relative in arb_relative_path(),
            working_dir in arb_working_dir(),
        ) {
            let result = resolve_relative(&relative, &working_dir);

            // Result should start with the working directory
            let result_str = result.to_string_lossy().to_string();
            let working_dir_str = working_dir.to_string_lossy().to_string();

            prop_assert!(result_str.starts_with(&working_dir_str),
                "Resolved path {:?} should start with working directory {:?}",
                result_str, working_dir_str);

            // Result should contain the relative path components after the working dir
            let suffix = &result_str[working_dir_str.len()..];
            // The suffix should be "/" followed by the relative path
            let expected_suffix = format!("/{}", relative);
            prop_assert_eq!(suffix, &expected_suffix,
                "Suffix after working dir should be '/{:?}', got '{:?}'",
                relative, suffix);
        }
    }
}
