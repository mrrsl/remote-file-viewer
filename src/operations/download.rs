// File download (view + copy)

use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

use tempfile::NamedTempFile;

use crate::ssh::SshClient;

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
