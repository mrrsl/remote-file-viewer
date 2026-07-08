// Direct path navigation logic

use std::path::{Path, PathBuf};

use crate::ssh::{SshClient, SshError};

/// Result of resolving a navigation target path via stat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigateTarget {
    /// The path is a directory — navigate into it.
    Directory(PathBuf),
    /// The path is a file — navigate to its parent directory and highlight the file.
    File { parent: PathBuf, filename: String },
}

/// Validate that the input is an absolute path (starts with '/').
pub fn validate_absolute_path(input: &str) -> bool {
    !input.is_empty() && input.starts_with('/')
}

/// Resolve a navigation target: stat the path to determine if it's a file or directory.
///
/// - If the path is a directory, returns `NavigateTarget::Directory`.
/// - If the path is a file or symlink, returns `NavigateTarget::File` with the parent
///   directory and filename extracted from the path.
/// - If stat fails, propagates the `SshError`.
/// - If `use_sudo` is true and SFTP stat returns PermissionDenied, retries with sudo.
pub fn resolve_navigate_target(
    ssh: &SshClient,
    path: &Path,
    use_sudo: bool,
) -> Result<NavigateTarget, SshError> {
    let is_dir = match ssh.stat(path) {
        Ok(stat) => stat.is_dir(),
        Err(SshError::PermissionDenied(_)) if use_sudo => {
            let (is_dir, _) = ssh.sudo_stat(path)?;
            is_dir
        }
        Err(e) => return Err(e),
    };

    if is_dir {
        Ok(NavigateTarget::Directory(path.to_path_buf()))
    } else {
        let parent = path.parent().unwrap_or(Path::new("/")).to_path_buf();
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok(NavigateTarget::File { parent, filename })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_absolute_path_valid() {
        assert!(validate_absolute_path("/"));
        assert!(validate_absolute_path("/home"));
        assert!(validate_absolute_path("/home/user/file.txt"));
        assert!(validate_absolute_path("/var/log/kafka"));
    }

    #[test]
    fn test_validate_absolute_path_invalid() {
        assert!(!validate_absolute_path(""));
        assert!(!validate_absolute_path("home/user"));
        assert!(!validate_absolute_path("relative/path"));
        assert!(!validate_absolute_path("./local"));
        assert!(!validate_absolute_path("../parent"));
        assert!(!validate_absolute_path(" /space-prefixed"));
    }

    #[test]
    fn test_validate_absolute_path_edge_cases() {
        assert!(validate_absolute_path("/a"));
        assert!(!validate_absolute_path("a/"));
        assert!(validate_absolute_path("//double-slash"));
        assert!(validate_absolute_path("/path with spaces/file"));
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    // Feature: ssh-remote-file-browser, Property 11: Absolute path validation
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Validates: Requirements 8.2**
        ///
        /// For arbitrary non-empty strings: validate_absolute_path returns true
        /// iff the string starts with '/'.
        #[test]
        fn prop_absolute_path_validation(input in ".+") {
            let result = validate_absolute_path(&input);
            let expected = !input.is_empty() && input.starts_with('/');
            prop_assert_eq!(result, expected,
                "validate_absolute_path({:?}) returned {} but expected {}",
                input, result, expected
            );
        }

        /// **Validates: Requirements 8.2**
        ///
        /// Strings starting with '/' should always return true.
        #[test]
        fn prop_absolute_path_with_slash_prefix(suffix in ".*") {
            let input = format!("/{}", suffix);
            prop_assert!(validate_absolute_path(&input),
                "validate_absolute_path({:?}) should return true for path starting with '/'",
                input
            );
        }

        /// **Validates: Requirements 8.2**
        ///
        /// Non-empty strings NOT starting with '/' should always return false.
        #[test]
        fn prop_non_absolute_path(input in "[^/].*") {
            prop_assert!(!validate_absolute_path(&input),
                "validate_absolute_path({:?}) should return false for path not starting with '/'",
                input
            );
        }
    }

    /// **Validates: Requirements 8.2**
    ///
    /// Empty string should always return false.
    #[test]
    fn test_empty_string_returns_false() {
        assert!(!validate_absolute_path(""), "empty string should return false");
    }
}
