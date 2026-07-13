// File download (view + copy)

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::NamedTempFile;

use crate::ssh::SshClient;
use crate::types::{DirectoryEntry, EntryType};

/// Maximum file size allowed for viewing (50 MB).
const MAX_VIEW_SIZE: u64 = 50 * 1024 * 1024;

/// Determine the preferred editor command.
///
/// Checks the EDITOR environment variable first, then VISUAL,
/// and falls back to "less" if neither is set.
pub fn get_editor() -> String {
    if let Ok(editor) = env::var("EDITOR") {
        if !editor.is_empty() {
            return editor;
        }
    }
    if let Ok(visual) = env::var("VISUAL") {
        if !visual.is_empty() {
            return visual;
        }
    }
    "less".to_string()
}

/// Download a remote file to a temporary local file.
///
/// Checks the file size against `max_size` before downloading.
/// Returns the NamedTempFile on success. The temp file is automatically
/// deleted when dropped (on error or when the caller is done with it).
pub fn download_to_temp(
    ssh: &SshClient,
    remote_path: &Path,
    max_size: u64,
) -> Result<NamedTempFile, String> {
    // Check file size before downloading
    let stat = ssh.stat(remote_path).map_err(|e| {
        format!("Cannot read file info: {}", e)
    })?;

    let file_size = stat.size.unwrap_or(0);
    if file_size > max_size {
        let size_mb = file_size as f64 / (1024.0 * 1024.0);
        return Err(format!(
            "File too large ({:.1} MB). Maximum allowed size is {} MB.",
            size_mb,
            max_size / (1024 * 1024)
        ));
    }

    // Create temp file
    let mut temp_file = NamedTempFile::new().map_err(|e| {
        format!("Failed to create temporary file: {}", e)
    })?;

    // Download to temp file
    ssh.download_file(remote_path, &mut temp_file).map_err(|e| {
        // temp_file is dropped here, auto-deleting the partial file
        format!("Download failed: {}", e)
    })?;

    // Flush to ensure all data is written
    temp_file.flush().map_err(|e| {
        format!("Failed to write temporary file: {}", e)
    })?;

    Ok(temp_file)
}

/// Open a file in the user's preferred editor.
///
/// Spawns the editor process and waits for it to exit.
/// Returns an error message if the editor cannot be launched.
pub fn open_in_editor(file_path: &Path) -> Result<(), String> {
    let editor = get_editor();

    let mut child = Command::new(&editor)
        .arg(file_path)
        .spawn()
        .map_err(|e| {
            format!("Failed to launch editor '{}': {}", editor, e)
        })?;

    child.wait().map_err(|e| {
        format!("Editor process error: {}", e)
    })?;

    Ok(())
}

/// View a remote file by downloading to a temp file and opening in the editor.
///
/// This is a convenience function that combines `download_to_temp` and `open_in_editor`.
/// The caller is responsible for suspending/resuming the TUI before/after calling this.
/// The temporary file is automatically deleted when this function returns.
pub fn view_file(ssh: &SshClient, remote_path: &Path) -> Result<(), String> {
    let temp_file = download_to_temp(ssh, remote_path, MAX_VIEW_SIZE)?;

    let result = open_in_editor(temp_file.path());

    // temp_file is dropped here, which deletes the temporary file
    result
}

/// Validate that the destination path's parent directory exists.
///
/// Returns `Ok(())` if the parent directory exists and is a directory.
/// Returns `Err(message)` if the parent does not exist or is not a directory.
pub fn validate_copy_destination(local_path: &Path) -> Result<(), String> {
    let parent = local_path.parent().ok_or_else(|| {
        "Cannot determine parent directory of destination path".to_string()
    })?;

    // An empty parent means the path is just a filename (e.g., "file.txt"),
    // which resolves to the current directory — that always exists.
    if parent.as_os_str().is_empty() {
        return Ok(());
    }

    if !parent.exists() {
        return Err(format!(
            "Parent directory does not exist: {}",
            parent.display()
        ));
    }

    if !parent.is_dir() {
        return Err(format!(
            "Parent path is not a directory: {}",
            parent.display()
        ));
    }

    Ok(())
}

/// Copy a remote file to a local path. Returns bytes transferred.
///
/// On error, any partially written local file is deleted before returning
/// the error message. On success, returns the total number of bytes written.
pub fn copy_remote_file(
    ssh: &SshClient,
    remote_path: &Path,
    local_path: &Path,
) -> Result<u64, String> {
    // Create/open the local file for writing
    let mut file = fs::File::create(local_path).map_err(|e| {
        format!("Failed to create local file '{}': {}", local_path.display(), e)
    })?;

    // Download from SSH to the local file
    let result = ssh.download_file(remote_path, &mut file);

    match result {
        Ok(bytes_written) => Ok(bytes_written),
        Err(ssh_err) => {
            // Delete the partial file on error
            let _ = fs::remove_file(local_path);
            Err(format!("File transfer failed: {}", ssh_err))
        }
    }
}

/// Summary of a completed recursive directory download.
#[derive(Debug)]
pub struct DirCopyResult {
    /// Total bytes successfully written to disk.
    pub bytes_transferred: u64,
    /// Number of files successfully downloaded.
    pub files_copied: usize,
    /// Files that failed with path and error reason.
    pub failures: Vec<(PathBuf, String)>,
    /// Count of symlink entries skipped.
    pub symlinks_skipped: usize,
}

/// Trait abstracting remote filesystem operations for testability.
///
/// The recursive walk logic uses this trait so that property tests can
/// inject mock implementations without requiring an actual SSH connection.
pub(crate) trait RemoteFs {
    fn list_dir(&self, path: &Path) -> Result<Vec<DirectoryEntry>, String>;
    fn download_file(&self, remote_path: &Path, local_path: &Path) -> Result<u64, String>;
}

/// Implementation of `RemoteFs` for `SshClient`.
struct SshRemoteFs<'a> {
    ssh: &'a SshClient,
}

impl<'a> RemoteFs for SshRemoteFs<'a> {
    fn list_dir(&self, path: &Path) -> Result<Vec<DirectoryEntry>, String> {
        self.ssh
            .list_dir(path)
            .map_err(|e| format!("{}", e))
    }

    fn download_file(&self, remote_path: &Path, local_path: &Path) -> Result<u64, String> {
        let mut file = fs::File::create(local_path).map_err(|e| {
            format!("Failed to create local file '{}': {}", local_path.display(), e)
        })?;

        match self.ssh.download_file(remote_path, &mut file) {
            Ok(bytes) => Ok(bytes),
            Err(ssh_err) => {
                drop(file);
                let _ = fs::remove_file(local_path);
                Err(format!("File transfer failed: {}", ssh_err))
            }
        }
    }
}

/// Copy a remote directory recursively to a local path.
///
/// Creates the local root directory, lists the remote directory tree via
/// recursive `list_dir` calls, downloads each file, skips symlinks, and
/// records individual failures without aborting the walk.
///
/// Returns `Err` only on total failure (cannot create local root or list
/// the top-level remote directory). Otherwise returns `Ok(DirCopyResult)`
/// with per-file outcomes.
pub fn copy_remote_directory(
    ssh: &SshClient,
    remote_path: &Path,
    local_path: &Path,
) -> Result<DirCopyResult, String> {
    let fs_ops = SshRemoteFs { ssh };
    copy_remote_directory_with_fs(&fs_ops, remote_path, local_path)
}

/// Internal recursive walk using a `RemoteFs` trait object.
///
/// This is separated from `copy_remote_directory` to enable property-based
/// testing with mock implementations.
pub(crate) fn copy_remote_directory_with_fs(
    remote_fs: &dyn RemoteFs,
    remote_path: &Path,
    local_path: &Path,
) -> Result<DirCopyResult, String> {
    // Step 1: Create local root directory
    fs::create_dir_all(local_path).map_err(|e| {
        format!(
            "Failed to create local directory '{}': {}",
            local_path.display(),
            e
        )
    })?;

    // Step 2: List top-level remote directory (total failure if this fails)
    let top_entries = remote_fs.list_dir(remote_path).map_err(|e| {
        format!(
            "Failed to list remote directory '{}': {}",
            remote_path.display(),
            e
        )
    })?;

    let mut result = DirCopyResult {
        bytes_transferred: 0,
        files_copied: 0,
        failures: Vec::new(),
        symlinks_skipped: 0,
    };

    // Step 3: BFS traversal using a stack of (remote_dir_path, local_dir_path, entries)
    // We process top_entries first, then any subdirectories discovered.
    let mut stack: Vec<(PathBuf, PathBuf, Vec<DirectoryEntry>)> = Vec::new();
    stack.push((remote_path.to_path_buf(), local_path.to_path_buf(), top_entries));

    while let Some((_, local_dir, entries)) = stack.pop() {
        for entry in entries {
            match entry.entry_type {
                EntryType::Symlink => {
                    // Skip symlinks per requirement 6.4
                    result.symlinks_skipped += 1;
                }
                EntryType::Directory => {
                    let local_subdir = local_dir.join(&entry.name);

                    // Create local subdirectory
                    if let Err(e) = fs::create_dir_all(&local_subdir) {
                        result.failures.push((
                            entry.path.clone(),
                            format!("Failed to create local directory '{}': {}", local_subdir.display(), e),
                        ));
                        // Skip this subtree
                        continue;
                    }

                    // List remote subdirectory
                    match remote_fs.list_dir(&entry.path) {
                        Ok(sub_entries) => {
                            stack.push((entry.path.clone(), local_subdir, sub_entries));
                        }
                        Err(e) => {
                            // Record failure for this subdirectory and skip subtree
                            result.failures.push((
                                entry.path.clone(),
                                format!("Failed to list remote directory: {}", e),
                            ));
                        }
                    }
                }
                EntryType::File => {
                    let local_file_path = local_dir.join(&entry.name);

                    match remote_fs.download_file(&entry.path, &local_file_path) {
                        Ok(bytes) => {
                            result.bytes_transferred += bytes;
                            result.files_copied += 1;
                        }
                        Err(e) => {
                            // delete partial file (RemoteFs impl handles this,
                            // but ensure cleanup in case of local-only failures)
                            let _ = fs::remove_file(&local_file_path);
                            result.failures.push((entry.path.clone(), e));
                        }
                    }
                }
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_editor_fallback() {
        // Clear both env vars to test fallback
        unsafe {
            env::remove_var("EDITOR");
            env::remove_var("VISUAL");
        }
        assert_eq!(get_editor(), "less");
    }

    #[test]
    fn test_get_editor_from_editor_var() {
        unsafe {
            env::set_var("EDITOR", "vim");
            env::remove_var("VISUAL");
        }
        assert_eq!(get_editor(), "vim");
        unsafe { env::remove_var("EDITOR"); }
    }

    #[test]
    fn test_get_editor_from_visual_var() {
        unsafe {
            env::remove_var("EDITOR");
            env::set_var("VISUAL", "code");
        }
        assert_eq!(get_editor(), "code");
        unsafe { env::remove_var("VISUAL"); }
    }

    #[test]
    fn test_get_editor_prefers_editor_over_visual() {
        unsafe {
            env::set_var("EDITOR", "nano");
            env::set_var("VISUAL", "code");
        }
        assert_eq!(get_editor(), "nano");
        unsafe {
            env::remove_var("EDITOR");
            env::remove_var("VISUAL");
        }
    }

    #[test]
    fn test_get_editor_skips_empty_editor() {
        unsafe {
            env::set_var("EDITOR", "");
            env::set_var("VISUAL", "code");
        }
        assert_eq!(get_editor(), "code");
        unsafe {
            env::remove_var("EDITOR");
            env::remove_var("VISUAL");
        }
    }

    #[test]
    fn test_get_editor_skips_empty_both() {
        unsafe {
            env::set_var("EDITOR", "");
            env::set_var("VISUAL", "");
        }
        assert_eq!(get_editor(), "less");
        unsafe {
            env::remove_var("EDITOR");
            env::remove_var("VISUAL");
        }
    }

    #[test]
    fn test_open_in_editor_invalid_command() {
        // Use a command that definitely doesn't exist
        unsafe { env::set_var("EDITOR", "nonexistent_editor_xyz_12345"); }
        let temp = NamedTempFile::new().unwrap();
        let result = open_in_editor(temp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to launch editor"));
        unsafe { env::remove_var("EDITOR"); }
    }
}

#[cfg(test)]
mod dir_copy_tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// A mock implementation of RemoteFs for testing the recursive walk.
    struct MockRemoteFs {
        /// Maps remote directory paths to their entries.
        dirs: HashMap<PathBuf, Vec<DirectoryEntry>>,
        /// Maps remote file paths to their content size.
        /// If the value is Err, the download will fail.
        files: HashMap<PathBuf, Result<u64, String>>,
    }

    impl RemoteFs for MockRemoteFs {
        fn list_dir(&self, path: &Path) -> Result<Vec<DirectoryEntry>, String> {
            self.dirs
                .get(path)
                .cloned()
                .ok_or_else(|| format!("No such directory: {}", path.display()))
        }

        fn download_file(&self, remote_path: &Path, local_path: &Path) -> Result<u64, String> {
            match self.files.get(remote_path) {
                Some(Ok(bytes)) => {
                    // Write dummy content to the local file
                    fs::write(local_path, vec![0u8; *bytes as usize])
                        .map_err(|e| format!("Write failed: {}", e))?;
                    Ok(*bytes)
                }
                Some(Err(e)) => Err(e.clone()),
                None => Err(format!("File not found: {}", remote_path.display())),
            }
        }
    }

    fn make_entry(name: &str, path: &str, entry_type: EntryType, size: u64) -> DirectoryEntry {
        DirectoryEntry {
            name: name.to_string(),
            path: PathBuf::from(path),
            entry_type,
            size,
        }
    }

    #[test]
    fn test_copy_empty_directory() {
        let tmp = tempfile::TempDir::new().unwrap();
        let local_path = tmp.path().join("dest");

        let mock = MockRemoteFs {
            dirs: HashMap::from([(PathBuf::from("/remote/dir"), vec![])]),
            files: HashMap::new(),
        };

        let result =
            copy_remote_directory_with_fs(&mock, Path::new("/remote/dir"), &local_path).unwrap();

        assert_eq!(result.bytes_transferred, 0);
        assert_eq!(result.files_copied, 0);
        assert_eq!(result.symlinks_skipped, 0);
        assert!(result.failures.is_empty());
        assert!(local_path.exists());
    }

    #[test]
    fn test_copy_files_and_skip_symlinks() {
        let tmp = tempfile::TempDir::new().unwrap();
        let local_path = tmp.path().join("dest");

        let mock = MockRemoteFs {
            dirs: HashMap::from([(
                PathBuf::from("/remote/dir"),
                vec![
                    make_entry("file1.txt", "/remote/dir/file1.txt", EntryType::File, 100),
                    make_entry("link1", "/remote/dir/link1", EntryType::Symlink, 0),
                    make_entry("file2.txt", "/remote/dir/file2.txt", EntryType::File, 200),
                ],
            )]),
            files: HashMap::from([
                (PathBuf::from("/remote/dir/file1.txt"), Ok(100)),
                (PathBuf::from("/remote/dir/file2.txt"), Ok(200)),
            ]),
        };

        let result =
            copy_remote_directory_with_fs(&mock, Path::new("/remote/dir"), &local_path).unwrap();

        assert_eq!(result.bytes_transferred, 300);
        assert_eq!(result.files_copied, 2);
        assert_eq!(result.symlinks_skipped, 1);
        assert!(result.failures.is_empty());
        assert!(local_path.join("file1.txt").exists());
        assert!(local_path.join("file2.txt").exists());
    }

    #[test]
    fn test_copy_nested_directories() {
        let tmp = tempfile::TempDir::new().unwrap();
        let local_path = tmp.path().join("dest");

        let mock = MockRemoteFs {
            dirs: HashMap::from([
                (
                    PathBuf::from("/remote/dir"),
                    vec![
                        make_entry("sub", "/remote/dir/sub", EntryType::Directory, 0),
                        make_entry("root.txt", "/remote/dir/root.txt", EntryType::File, 50),
                    ],
                ),
                (
                    PathBuf::from("/remote/dir/sub"),
                    vec![make_entry(
                        "nested.txt",
                        "/remote/dir/sub/nested.txt",
                        EntryType::File,
                        75,
                    )],
                ),
            ]),
            files: HashMap::from([
                (PathBuf::from("/remote/dir/root.txt"), Ok(50)),
                (PathBuf::from("/remote/dir/sub/nested.txt"), Ok(75)),
            ]),
        };

        let result =
            copy_remote_directory_with_fs(&mock, Path::new("/remote/dir"), &local_path).unwrap();

        assert_eq!(result.bytes_transferred, 125);
        assert_eq!(result.files_copied, 2);
        assert!(local_path.join("sub").is_dir());
        assert!(local_path.join("sub/nested.txt").exists());
    }

    #[test]
    fn test_copy_records_file_failures_and_continues() {
        let tmp = tempfile::TempDir::new().unwrap();
        let local_path = tmp.path().join("dest");

        let mock = MockRemoteFs {
            dirs: HashMap::from([(
                PathBuf::from("/remote/dir"),
                vec![
                    make_entry("good.txt", "/remote/dir/good.txt", EntryType::File, 100),
                    make_entry("bad.txt", "/remote/dir/bad.txt", EntryType::File, 50),
                    make_entry("also_good.txt", "/remote/dir/also_good.txt", EntryType::File, 200),
                ],
            )]),
            files: HashMap::from([
                (PathBuf::from("/remote/dir/good.txt"), Ok(100)),
                (
                    PathBuf::from("/remote/dir/bad.txt"),
                    Err("Permission denied".to_string()),
                ),
                (PathBuf::from("/remote/dir/also_good.txt"), Ok(200)),
            ]),
        };

        let result =
            copy_remote_directory_with_fs(&mock, Path::new("/remote/dir"), &local_path).unwrap();

        assert_eq!(result.bytes_transferred, 300);
        assert_eq!(result.files_copied, 2);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].0, PathBuf::from("/remote/dir/bad.txt"));
        assert!(result.failures[0].1.contains("Permission denied"));
        // Partial file should be cleaned up
        assert!(!local_path.join("bad.txt").exists());
    }

    #[test]
    fn test_copy_fails_when_top_level_list_fails() {
        let tmp = tempfile::TempDir::new().unwrap();
        let local_path = tmp.path().join("dest");

        let mock = MockRemoteFs {
            dirs: HashMap::new(), // No directories registered
            files: HashMap::new(),
        };

        let result =
            copy_remote_directory_with_fs(&mock, Path::new("/remote/dir"), &local_path);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to list remote directory"));
    }

    #[test]
    fn test_copy_subdirectory_list_failure_recorded_and_continues() {
        let tmp = tempfile::TempDir::new().unwrap();
        let local_path = tmp.path().join("dest");

        let mock = MockRemoteFs {
            dirs: HashMap::from([(
                PathBuf::from("/remote/dir"),
                vec![
                    make_entry("sub_ok", "/remote/dir/sub_ok", EntryType::Directory, 0),
                    make_entry("sub_fail", "/remote/dir/sub_fail", EntryType::Directory, 0),
                    make_entry("file.txt", "/remote/dir/file.txt", EntryType::File, 10),
                ],
            ),
            (
                PathBuf::from("/remote/dir/sub_ok"),
                vec![make_entry("inner.txt", "/remote/dir/sub_ok/inner.txt", EntryType::File, 20)],
            ),
            // sub_fail is NOT in dirs, so list_dir will fail
            ]),
            files: HashMap::from([
                (PathBuf::from("/remote/dir/file.txt"), Ok(10)),
                (PathBuf::from("/remote/dir/sub_ok/inner.txt"), Ok(20)),
            ]),
        };

        let result =
            copy_remote_directory_with_fs(&mock, Path::new("/remote/dir"), &local_path).unwrap();

        // file.txt and sub_ok/inner.txt should succeed
        assert_eq!(result.files_copied, 2);
        assert_eq!(result.bytes_transferred, 30);
        // sub_fail should be recorded as a failure
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].0, PathBuf::from("/remote/dir/sub_fail"));
    }
}

#[cfg(test)]
mod copy_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_validate_copy_destination_valid_parent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("output.log");
        assert!(validate_copy_destination(&dest).is_ok());
    }

    #[test]
    fn test_validate_copy_destination_nonexistent_parent() {
        let path = PathBuf::from("/nonexistent_dir_xyz_9999/file.txt");
        let result = validate_copy_destination(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Parent directory does not exist"));
    }

    #[test]
    fn test_validate_copy_destination_parent_is_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let fake_dir = tmp.path().join("not_a_dir");
        fs::write(&fake_dir, "content").unwrap();

        let dest = fake_dir.join("file.txt");
        let result = validate_copy_destination(&dest);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a directory"));
    }

    #[test]
    fn test_validate_copy_destination_bare_filename() {
        // A bare filename like "output.log" has an empty parent, which is fine
        let dest = PathBuf::from("output.log");
        assert!(validate_copy_destination(&dest).is_ok());
    }

    #[test]
    fn test_validate_copy_destination_nested_in_existing_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let subdir = tmp.path().join("subdir");
        fs::create_dir(&subdir).unwrap();

        let dest = subdir.join("output.log");
        assert!(validate_copy_destination(&dest).is_ok());
    }

    #[test]
    fn test_validate_copy_destination_nested_nonexistent_parent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dest = tmp.path().join("missing_subdir").join("output.log");
        let result = validate_copy_destination(&dest);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Parent directory does not exist"));
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// A mock implementation of RemoteFs for property tests.
    struct MockRemoteFs {
        /// Maps remote directory paths to their entries.
        dirs: HashMap<PathBuf, Vec<DirectoryEntry>>,
        /// Maps remote file paths to download result.
        /// Ok(bytes) for success, Err(msg) for failure.
        files: HashMap<PathBuf, Result<u64, String>>,
    }

    impl RemoteFs for MockRemoteFs {
        fn list_dir(&self, path: &Path) -> Result<Vec<DirectoryEntry>, String> {
            self.dirs
                .get(path)
                .cloned()
                .ok_or_else(|| format!("No such directory: {}", path.display()))
        }

        fn download_file(&self, remote_path: &Path, local_path: &Path) -> Result<u64, String> {
            match self.files.get(remote_path) {
                Some(Ok(bytes)) => {
                    fs::write(local_path, vec![0u8; *bytes as usize])
                        .map_err(|e| format!("Write failed: {}", e))?;
                    Ok(*bytes)
                }
                Some(Err(e)) => Err(e.clone()),
                None => Err(format!("File not found: {}", remote_path.display())),
            }
        }
    }

    /// Strategy to generate a flat list of entries with mixed types.
    fn entry_strategy() -> impl Strategy<Value = Vec<(String, EntryType)>> {
        prop::collection::vec(
            (
                "[a-zA-Z0-9_]{1,12}",
                prop_oneof![
                    Just(EntryType::File),
                    Just(EntryType::Directory),
                    Just(EntryType::Symlink),
                ],
            ),
            1..=10,
        )
        .prop_filter("unique names", |entries| {
            let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
            let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
            unique.len() == names.len()
        })
    }

    /// Build a MockRemoteFs from a flat list of entries under /remote/root.
    fn build_mock(
        entries: &[(String, EntryType)],
        fail_set: &std::collections::HashSet<String>,
    ) -> MockRemoteFs {
        let root = PathBuf::from("/remote/root");
        let mut dir_entries = Vec::new();
        let mut dirs: HashMap<PathBuf, Vec<DirectoryEntry>> = HashMap::new();
        let mut files: HashMap<PathBuf, Result<u64, String>> = HashMap::new();

        for (name, entry_type) in entries {
            let entry_path = root.join(name);
            dir_entries.push(DirectoryEntry {
                name: name.clone(),
                path: entry_path.clone(),
                entry_type: entry_type.clone(),
                size: 64,
            });

            match entry_type {
                EntryType::File => {
                    if fail_set.contains(name) {
                        files.insert(entry_path, Err("Injected failure".to_string()));
                    } else {
                        files.insert(entry_path, Ok(64));
                    }
                }
                EntryType::Directory => {
                    // Empty subdirectory
                    dirs.insert(entry_path, vec![]);
                }
                EntryType::Symlink => {
                    // nothing to register
                }
            }
        }

        dirs.insert(root, dir_entries);
        MockRemoteFs { dirs, files }
    }

    // Feature: arrow-nav-and-dir-download, Property 4: Walk visits files, skips symlinks
    // Validates: Requirements 6.3, 6.4
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn walk_visits_all_files_and_skips_symlinks(entries in entry_strategy()) {
            let tmp = tempfile::TempDir::new().unwrap();
            let local_path = tmp.path().join("dest");

            let fail_set = std::collections::HashSet::new();
            let mock = build_mock(&entries, &fail_set);

            let result = copy_remote_directory_with_fs(
                &mock,
                Path::new("/remote/root"),
                &local_path,
            ).unwrap();

            let file_count = entries.iter().filter(|(_, t)| *t == EntryType::File).count();
            let symlink_count = entries.iter().filter(|(_, t)| *t == EntryType::Symlink).count();

            prop_assert_eq!(
                result.files_copied, file_count,
                "Expected {} files copied, got {}", file_count, result.files_copied
            );
            prop_assert_eq!(
                result.symlinks_skipped, symlink_count,
                "Expected {} symlinks skipped, got {}", symlink_count, result.symlinks_skipped
            );
            prop_assert!(
                result.failures.is_empty(),
                "Expected no failures, got {:?}", result.failures
            );
        }
    }

    // Feature: arrow-nav-and-dir-download, Property 5: Walk mirrors dir structure
    // Validates: Requirements 6.2
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn walk_mirrors_directory_structure(entries in entry_strategy()) {
            let tmp = tempfile::TempDir::new().unwrap();
            let local_path = tmp.path().join("dest");

            let fail_set = std::collections::HashSet::new();
            let mock = build_mock(&entries, &fail_set);

            let _result = copy_remote_directory_with_fs(
                &mock,
                Path::new("/remote/root"),
                &local_path,
            ).unwrap();

            // Verify each directory entry has a corresponding local directory
            for (name, entry_type) in &entries {
                if *entry_type == EntryType::Directory {
                    let local_dir = local_path.join(name);
                    prop_assert!(
                        local_dir.is_dir(),
                        "Expected local directory '{}' to exist", local_dir.display()
                    );
                }
            }
        }
    }

    /// Strategy for failure injection: pick a subset of file names to fail.
    fn failure_injection_strategy() -> impl Strategy<Value = (Vec<(String, EntryType)>, std::collections::HashSet<String>)> {
        entry_strategy().prop_flat_map(|entries| {
            let file_names: Vec<String> = entries
                .iter()
                .filter(|(_, t)| *t == EntryType::File)
                .map(|(n, _)| n.clone())
                .collect();
            let num_files = file_names.len();
            let entries_clone = entries.clone();

            // Generate a subset of file indices to fail
            prop::collection::vec(prop::bool::ANY, num_files).prop_map(move |flags| {
                let fail_set: std::collections::HashSet<String> = file_names
                    .iter()
                    .zip(flags.iter())
                    .filter(|(_, fail)| **fail)
                    .map(|(name, _)| name.clone())
                    .collect();
                (entries_clone.clone(), fail_set)
            })
        })
    }

    // Feature: arrow-nav-and-dir-download, Property 6: Failures don't halt walk
    // Validates: Requirements 7.1
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn failures_dont_halt_walk((entries, fail_set) in failure_injection_strategy()) {
            let tmp = tempfile::TempDir::new().unwrap();
            let local_path = tmp.path().join("dest");

            let mock = build_mock(&entries, &fail_set);

            let result = copy_remote_directory_with_fs(
                &mock,
                Path::new("/remote/root"),
                &local_path,
            ).unwrap();

            let total_files = entries.iter().filter(|(_, t)| *t == EntryType::File).count();
            let expected_failures = fail_set.len();
            let expected_success = total_files - expected_failures;

            prop_assert_eq!(
                result.files_copied, expected_success,
                "Expected {} successful copies, got {}", expected_success, result.files_copied
            );
            prop_assert_eq!(
                result.failures.len(), expected_failures,
                "Expected {} failures, got {}", expected_failures, result.failures.len()
            );
            // bytes_transferred should only count successful files
            let expected_bytes = (expected_success as u64) * 64;
            prop_assert_eq!(
                result.bytes_transferred, expected_bytes,
                "Expected {} bytes transferred, got {}", expected_bytes, result.bytes_transferred
            );
        }
    }
}
