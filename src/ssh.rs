// SSH session and SFTP wrapper

use std::fmt;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ssh2::{FileStat, Session, Sftp};

use crate::config::AppConfig;
use crate::types::{DirectoryEntry, EntryType};

/// SSH client wrapping an active session and SFTP channel.
pub struct SshClient {
    session: Session,
    sftp: Sftp,
}

/// Errors that can occur during SSH operations.
#[derive(Debug)]
pub enum SshError {
    ConnectionFailed(String),
    AuthenticationFailed(String),
    Timeout,
    PermissionDenied(PathBuf),
    ConnectionLost,
    IoError(io::Error),
}

impl fmt::Display for SshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SshError::ConnectionFailed(msg) => {
                write!(f, "Connection failed: {}", msg)
            }
            SshError::AuthenticationFailed(msg) => {
                write!(f, "Authentication failed: {}", msg)
            }
            SshError::Timeout => {
                write!(f, "Operation timed out")
            }
            SshError::PermissionDenied(path) => {
                write!(f, "Permission denied: {}", path.display())
            }
            SshError::ConnectionLost => {
                write!(f, "Connection lost")
            }
            SshError::IoError(err) => {
                write!(f, "I/O error: {}", err)
            }
        }
    }
}

impl From<io::Error> for SshError {
    fn from(err: io::Error) -> Self {
        if err.kind() == io::ErrorKind::TimedOut {
            SshError::Timeout
        } else {
            SshError::IoError(err)
        }
    }
}

impl SshClient {
    /// Connect to a remote server using the provided config.
    ///
    /// Establishes a TCP connection, performs SSH handshake, authenticates
    /// with public key from the config's identity file, and opens an SFTP channel.
    /// Timeout is set to 30 seconds.
    pub fn connect(config: &AppConfig) -> Result<Self, SshError> {
        let addr = format!("{}:22", config.ip_address);

        // Establish TCP connection with 30-second timeout
        let tcp = TcpStream::connect_timeout(
            &addr.parse().map_err(|e| {
                SshError::ConnectionFailed(format!("Invalid address '{}': {}", addr, e))
            })?,
            Duration::from_secs(30),
        )
        .map_err(|e| {
            if e.kind() == io::ErrorKind::TimedOut {
                SshError::Timeout
            } else {
                SshError::ConnectionFailed(format!("Cannot connect to {}: {}", addr, e))
            }
        })?;

        // Create SSH session
        let mut session = Session::new()
            .map_err(|e| SshError::ConnectionFailed(format!("Failed to create session: {}", e)))?;

        // Set 30-second timeout on the session
        session.set_timeout(30_000);
        session.set_tcp_stream(tcp);

        // Perform SSH handshake
        session.handshake().map_err(|e| {
            SshError::ConnectionFailed(format!("SSH handshake failed: {}", e))
        })?;

        // Authenticate with public key
        session
            .userauth_pubkey_file(
                &config.username,
                None,
                &config.ssh_identity_file,
                None,
            )
            .map_err(|e| {
                SshError::AuthenticationFailed(format!(
                    "Public key authentication failed for user '{}': {}",
                    config.username, e
                ))
            })?;

        if !session.authenticated() {
            return Err(SshError::AuthenticationFailed(
                "Session not authenticated after key exchange".to_string(),
            ));
        }

        // Open SFTP channel
        let sftp = session.sftp().map_err(|e| {
            SshError::ConnectionFailed(format!("Failed to open SFTP channel: {}", e))
        })?;

        Ok(SshClient { session, sftp })
    }

    /// List directory entries at the given remote path.
    ///
    /// Returns a vector of `DirectoryEntry` items for each entry in the directory.
    pub fn list_dir(&self, path: &Path) -> Result<Vec<DirectoryEntry>, SshError> {
        let entries = self.sftp.readdir(path).map_err(|e| {
            Self::map_sftp_error(e, path)
        })?;

        let result = entries
            .into_iter()
            .map(|(entry_path, stat)| {
                let name = entry_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                let entry_type = if stat.is_dir() {
                    EntryType::Directory
                } else if stat.file_type().is_symlink() {
                    EntryType::Symlink
                } else {
                    EntryType::File
                };

                let size = stat.size.unwrap_or(0);

                DirectoryEntry {
                    name,
                    path: entry_path,
                    entry_type,
                    size,
                }
            })
            .collect();

        Ok(result)
    }

    /// Download a remote file, writing its contents to the provided writer.
    ///
    /// Returns the total number of bytes written.
    pub fn download_file(
        &self,
        remote_path: &Path,
        writer: &mut impl Write,
    ) -> Result<u64, SshError> {
        let mut file = self.sftp.open(remote_path).map_err(|e| {
            Self::map_sftp_error(e, remote_path)
        })?;

        let mut buf = [0u8; 8192];
        let mut total_bytes: u64 = 0;

        loop {
            let bytes_read = file.read(&mut buf).map_err(|e| {
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut {
                    SshError::Timeout
                } else {
                    SshError::IoError(e)
                }
            })?;

            if bytes_read == 0 {
                break;
            }

            writer.write_all(&buf[..bytes_read]).map_err(SshError::IoError)?;
            total_bytes += bytes_read as u64;
        }

        Ok(total_bytes)
    }

    /// Get file metadata (size, type, permissions) for a remote path.
    pub fn stat(&self, path: &Path) -> Result<FileStat, SshError> {
        self.sftp.stat(path).map_err(|e| Self::map_sftp_error(e, path))
    }

    /// Execute a recursive find command on the remote server.
    ///
    /// Runs `find <base> -name '*<pattern>*'` via a channel session
    /// and parses the output into `DirectoryEntry` items.
    pub fn find_recursive(
        &self,
        base: &Path,
        pattern: &str,
    ) -> Result<Vec<DirectoryEntry>, SshError> {
        let mut channel = self.session.channel_session().map_err(|e| {
            if !self.session.authenticated() {
                SshError::ConnectionLost
            } else {
                SshError::ConnectionFailed(format!("Failed to open channel: {}", e))
            }
        })?;

        // Sanitize pattern to prevent command injection
        let safe_pattern = pattern.replace('\'', "'\\''");
        let base_str = base.to_string_lossy();
        let command = format!(
            "find {} -name '*{}*' 2>/dev/null",
            shell_escape(&base_str),
            safe_pattern
        );

        channel.exec(&command).map_err(|e| {
            SshError::ConnectionFailed(format!("Failed to execute find command: {}", e))
        })?;

        let mut output = String::new();
        channel.read_to_string(&mut output).map_err(|e| {
            SshError::IoError(e)
        })?;

        channel.wait_close().ok();

        let mut entries = Vec::new();
        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let entry_path = PathBuf::from(line);
            let name = entry_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if name.is_empty() {
                continue;
            }

            // Try to stat each entry to determine its type and size
            let (entry_type, size) = match self.sftp.stat(&entry_path) {
                Ok(stat) => {
                    let etype = if stat.is_dir() {
                        EntryType::Directory
                    } else if stat.file_type().is_symlink() {
                        EntryType::Symlink
                    } else {
                        EntryType::File
                    };
                    (etype, stat.size.unwrap_or(0))
                }
                Err(_) => (EntryType::File, 0),
            };

            entries.push(DirectoryEntry {
                name,
                path: entry_path,
                entry_type,
                size,
            });
        }

        Ok(entries)
    }

    /// Check if the SSH connection is still alive.
    pub fn is_connected(&self) -> bool {
        self.session.authenticated()
    }

    /// Execute a shell command via SSH channel, returning stdout as a String.
    pub fn exec_command(&self, command: &str) -> Result<String, SshError> {
        let mut channel = self.session.channel_session().map_err(|e| {
            if !self.session.authenticated() {
                SshError::ConnectionLost
            } else {
                SshError::ConnectionFailed(format!("Failed to open channel: {}", e))
            }
        })?;

        channel.exec(command).map_err(|e| {
            SshError::ConnectionFailed(format!("Failed to execute command: {}", e))
        })?;

        let mut output = String::new();
        channel.read_to_string(&mut output).map_err(SshError::IoError)?;

        // Read stderr before closing
        let mut stderr_output = String::new();
        channel.stderr().read_to_string(&mut stderr_output).ok();

        channel.wait_close().ok();

        let exit_status = channel.exit_status().unwrap_or(-1);
        if exit_status != 0 {
            if stderr_output.contains("Permission denied") || stderr_output.contains("permission denied") {
                return Err(SshError::PermissionDenied(PathBuf::new()));
            }
            return Err(SshError::ConnectionFailed(
                format!("Command failed (exit {}): {}", exit_status, stderr_output.trim())
            ));
        }

        Ok(output)
    }

    /// List a directory using `sudo find -maxdepth 1 -printf`.
    pub fn sudo_list_dir(&self, path: &Path) -> Result<Vec<DirectoryEntry>, SshError> {
        let path_str = path.to_string_lossy();
        let cmd = format!(
            "sudo find {} -maxdepth 1 -mindepth 1 -printf '%y %s %p\\n'",
            shell_escape(&path_str)
        );
        let output = self.exec_command(&cmd)?;

        let mut entries = Vec::new();
        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }

            // Format: "d 4096 /path/to/dir" or "f 1234 /path/to/file"
            let mut parts = line.splitn(3, ' ');
            let type_char = parts.next().unwrap_or("");
            let size_str = parts.next().unwrap_or("0");
            let entry_path_str = parts.next().unwrap_or("");

            if entry_path_str.is_empty() { continue; }

            let entry_path = PathBuf::from(entry_path_str);
            let name = entry_path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let entry_type = match type_char {
                "d" => EntryType::Directory,
                "l" => EntryType::Symlink,
                _ => EntryType::File,
            };

            let size = size_str.parse::<u64>().unwrap_or(0);

            entries.push(DirectoryEntry { name, path: entry_path, entry_type, size });
        }

        Ok(entries)
    }

    /// Download a file using `sudo cat`.
    pub fn sudo_download_file(
        &self,
        remote_path: &Path,
        writer: &mut impl Write,
    ) -> Result<u64, SshError> {
        let path_str = remote_path.to_string_lossy();
        let cmd = format!("sudo cat {}", shell_escape(&path_str));

        let mut channel = self.session.channel_session().map_err(|e| {
            SshError::ConnectionFailed(format!("Failed to open channel: {}", e))
        })?;
        channel.exec(&cmd).map_err(|e| {
            SshError::ConnectionFailed(format!("Failed to execute command: {}", e))
        })?;

        let mut buf = [0u8; 8192];
        let mut total: u64 = 0;
        loop {
            let n = channel.read(&mut buf).map_err(SshError::IoError)?;
            if n == 0 { break; }
            writer.write_all(&buf[..n]).map_err(SshError::IoError)?;
            total += n as u64;
        }
        channel.wait_close().ok();
        Ok(total)
    }

    /// Stat a path using `sudo stat`, returns (is_directory, size).
    pub fn sudo_stat(&self, path: &Path) -> Result<(bool, u64), SshError> {
        let path_str = path.to_string_lossy();
        let cmd = format!(
            "sudo stat --format='%F %s' {}",
            shell_escape(&path_str)
        );
        let output = self.exec_command(&cmd)?;
        let trimmed = output.trim();

        let is_dir = trimmed.starts_with("directory");
        let size = trimmed.split_whitespace().last()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        Ok((is_dir, size))
    }

    /// Execute a recursive find command with sudo on the remote server.
    ///
    /// Runs `sudo find <base> -name '*<pattern>*'` via a channel session
    /// and parses the output into `DirectoryEntry` items.
    pub fn sudo_find_recursive(
        &self,
        base: &Path,
        pattern: &str,
    ) -> Result<Vec<DirectoryEntry>, SshError> {
        let safe_pattern = pattern.replace('\'', "'\\''");
        let base_str = base.to_string_lossy();
        let command = format!(
            "sudo find {} -name '*{}*' 2>/dev/null",
            shell_escape(&base_str),
            safe_pattern
        );

        let mut channel = self.session.channel_session().map_err(|e| {
            if !self.session.authenticated() {
                SshError::ConnectionLost
            } else {
                SshError::ConnectionFailed(format!("Failed to open channel: {}", e))
            }
        })?;

        channel.exec(&command).map_err(|e| {
            SshError::ConnectionFailed(format!("Failed to execute find command: {}", e))
        })?;

        let mut output = String::new();
        channel.read_to_string(&mut output).map_err(|e| {
            SshError::IoError(e)
        })?;

        channel.wait_close().ok();

        let mut entries = Vec::new();
        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let entry_path = PathBuf::from(line);
            let name = entry_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if name.is_empty() {
                continue;
            }

            // Try to stat each entry with sudo to determine its type and size
            let (entry_type, size) = match self.sudo_stat(&entry_path) {
                Ok((is_dir, sz)) => {
                    let etype = if is_dir {
                        EntryType::Directory
                    } else {
                        EntryType::File
                    };
                    (etype, sz)
                }
                Err(_) => (EntryType::File, 0),
            };

            entries.push(DirectoryEntry {
                name,
                path: entry_path,
                entry_type,
                size,
            });
        }

        Ok(entries)
    }

    /// Map an ssh2 error to an SshError, accounting for permission and timeout errors.
    fn map_sftp_error(err: ssh2::Error, path: &Path) -> SshError {
        match err.code() {
            ssh2::ErrorCode::Session(-9) => SshError::Timeout, // LIBSSH2_ERROR_TIMEOUT = -9
            ssh2::ErrorCode::SFTP(3) => {
                // LIBSSH2_FX_PERMISSION_DENIED = 3
                SshError::PermissionDenied(path.to_path_buf())
            }
            _ => {
                let msg = err.message();
                if msg.contains("permission") || msg.contains("Permission") {
                    SshError::PermissionDenied(path.to_path_buf())
                } else {
                    SshError::ConnectionFailed(format!("{}", err))
                }
            }
        }
    }
}

/// Simple shell escaping for a path used in a remote command.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
