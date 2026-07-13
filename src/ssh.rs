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

    /// Get total directory size in bytes via `du -sb <path>`.
    ///
    /// Uses `shell_escape` for path sanitization. Opens a channel session,
    /// executes the command, and parses the first token of output as a u64.
    pub fn dir_size(&self, path: &Path) -> Result<u64, SshError> {
        let mut channel = self.session.channel_session().map_err(|e| {
            if !self.session.authenticated() {
                SshError::ConnectionLost
            } else {
                SshError::ConnectionFailed(format!("Failed to open channel: {}", e))
            }
        })?;

        let path_str = path.to_string_lossy();
        let command = format!("du -sb {}", shell_escape(&path_str));

        channel.exec(&command).map_err(|e| {
            SshError::ConnectionFailed(format!("Failed to execute du command: {}", e))
        })?;

        let mut output = String::new();
        channel.read_to_string(&mut output).map_err(|e| {
            SshError::IoError(e)
        })?;

        channel.wait_close().ok();

        parse_du_output(&output)
    }

    /// Check if the SSH connection is still alive.
    pub fn is_connected(&self) -> bool {
        self.session.authenticated()
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

/// Parse the output of `du -sb` and return the byte count.
///
/// Expects the first whitespace-delimited token to be a valid u64.
/// Returns `SshError::ConnectionFailed` if the output is empty or unparseable.
pub fn parse_du_output(output: &str) -> Result<u64, SshError> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Err(SshError::ConnectionFailed(
            "Could not determine directory size: empty output".to_string(),
        ));
    }

    let token = trimmed.split_whitespace().next().unwrap_or("");
    token.parse::<u64>().map_err(|_| {
        SshError::ConnectionFailed(format!(
            "Could not parse directory size from: {}",
            token
        ))
    })
}

/// Simple shell escaping for a path used in a remote command.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_du_output_valid_tab_separated() {
        let result = parse_du_output("1234\t/some/path");
        assert_eq!(result.unwrap(), 1234);
    }

    #[test]
    fn parse_du_output_valid_spaces() {
        let result = parse_du_output("5678   /path/to/dir");
        assert_eq!(result.unwrap(), 5678);
    }

    #[test]
    fn parse_du_output_empty() {
        let result = parse_du_output("");
        assert!(result.is_err());
        match result.unwrap_err() {
            SshError::ConnectionFailed(msg) => {
                assert!(msg.contains("empty output"));
            }
            other => panic!("Expected ConnectionFailed, got: {:?}", other),
        }
    }

    #[test]
    fn parse_du_output_whitespace_only() {
        let result = parse_du_output("   \t\n  ");
        assert!(result.is_err());
        match result.unwrap_err() {
            SshError::ConnectionFailed(msg) => {
                assert!(msg.contains("empty output"));
            }
            other => panic!("Expected ConnectionFailed, got: {:?}", other),
        }
    }

    #[test]
    fn parse_du_output_invalid_number() {
        let result = parse_du_output("abc\t/path");
        assert!(result.is_err());
        match result.unwrap_err() {
            SshError::ConnectionFailed(msg) => {
                assert!(msg.contains("Could not parse directory size"));
            }
            other => panic!("Expected ConnectionFailed, got: {:?}", other),
        }
    }

    #[test]
    fn parse_du_output_only_number() {
        let result = parse_du_output("1234");
        assert_eq!(result.unwrap(), 1234);
    }

    #[test]
    fn parse_du_output_large_value() {
        let result = parse_du_output("18446744073709551615\t/big/dir");
        assert_eq!(result.unwrap(), u64::MAX);
    }

    #[test]
    fn parse_du_output_with_leading_newline() {
        let result = parse_du_output("\n9999\t/path");
        assert_eq!(result.unwrap(), 9999);
    }

    #[test]
    fn parse_du_output_with_trailing_newline() {
        let result = parse_du_output("4321\t/path\n");
        assert_eq!(result.unwrap(), 4321);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    // Feature: arrow-nav-and-dir-download, Property 1: du output parsing round-trip
    // Validates: Requirements 2.1, 2.2
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn du_parsing_round_trip(n: u64, p in "[a-zA-Z0-9_/. ]{1,100}") {
            let formatted = format!("{}\t{}", n, p);
            let result = parse_du_output(&formatted);
            prop_assert_eq!(result.unwrap(), n);
        }
    }
}
