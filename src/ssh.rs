// SSH session and SFTP wrapper

use std::fmt;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    /// On `PermissionDenied`, falls back to `sudo ls -la` via an exec channel.
    pub fn list_dir(&self, path: &Path) -> Result<Vec<DirectoryEntry>, SshError> {
        match self.sftp.readdir(path) {
            Ok(entries) => {
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
            Err(e) => {
                let err = Self::map_sftp_error(e, path);
                log_error("list_dir", &err);
                match err {
                    SshError::PermissionDenied(_) => self.sudo_ls_la(path),
                    _ => Err(err),
                }
            }
        }
    }

    /// Download a remote file, writing its contents to the provided writer.
    ///
    /// Returns the total number of bytes written.
    /// On PermissionDenied, falls back to `sudo cat` via an exec channel.
    pub fn download_file(
        &self,
        remote_path: &Path,
        writer: &mut impl Write,
    ) -> Result<u64, SshError> {
        let mut file = match self.sftp.open(remote_path) {
            Ok(f) => f,
            Err(e) => {
                let err = Self::map_sftp_error(e, remote_path);
                log_error("download_file", &err);
                return match err {
                    SshError::PermissionDenied(_) => self.sudo_cat(remote_path, writer),
                    other => Err(other),
                };
            }
        };

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

    /// Read a remote file via `sudo cat`, streaming its contents to the writer.
    ///
    /// Opens an exec channel, runs `sudo cat <escaped_path>`, streams stdout
    /// to the writer in 8 KiB chunks, and returns the total bytes written.
    /// Returns `PermissionDenied` if stderr indicates a password is required.
    fn sudo_cat(&self, path: &Path, writer: &mut impl Write) -> Result<u64, SshError> {
        let mut channel = self.session.channel_session().map_err(|e| {
            if !self.session.authenticated() {
                SshError::ConnectionLost
            } else {
                SshError::ConnectionFailed(format!("Failed to open channel: {}", e))
            }
        })?;

        let path_str = path.to_string_lossy();
        let command = format!("sudo cat {}", shell_escape(&path_str));

        channel.exec(&command).map_err(|e| {
            SshError::ConnectionFailed(format!("Failed to execute sudo cat command: {}", e))
        })?;

        // Stream stdout to writer in 8 KiB chunks
        let mut buf = [0u8; 8192];
        let mut total_bytes: u64 = 0;

        loop {
            let bytes_read = channel.read(&mut buf).map_err(|e| SshError::IoError(e))?;
            if bytes_read == 0 {
                break;
            }
            writer.write_all(&buf[..bytes_read]).map_err(SshError::IoError)?;
            total_bytes += bytes_read as u64;
        }

        // Read stderr after EOF to check for password prompts
        let mut stderr_output = String::new();
        channel
            .stderr()
            .read_to_string(&mut stderr_output)
            .map_err(|e| SshError::IoError(e))?;

        if stderr_output.contains("password is required") || stderr_output.contains("sudo:") {
            return Err(SshError::PermissionDenied(path.to_path_buf()));
        }

        // Check exit status
        channel.wait_close().ok();
        let exit_status = channel.exit_status().unwrap_or(-1);
        if exit_status != 0 {
            return Err(SshError::ConnectionFailed(format!(
                "sudo cat command failed with exit status {}",
                exit_status
            )));
        }

        Ok(total_bytes)
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

    /// Execute `sudo ls -la` on a remote path and parse the output into directory entries.
    ///
    /// Used as a fallback when SFTP readdir fails with PermissionDenied.
    fn sudo_ls_la(&self, path: &Path) -> Result<Vec<DirectoryEntry>, SshError> {
        let mut channel = self.session.channel_session().map_err(|e| {
            if !self.session.authenticated() {
                SshError::ConnectionLost
            } else {
                SshError::ConnectionFailed(format!("Failed to open channel: {}", e))
            }
        })?;

        let path_str = path.to_string_lossy();
        let command = format!("LC_ALL=C sudo ls -la {}", shell_escape(&path_str));

        channel.exec(&command).map_err(|e| {
            SshError::ConnectionFailed(format!("Failed to execute sudo ls -la command: {}", e))
        })?;

        // Read stdout
        let mut output = String::new();
        channel.read_to_string(&mut output).map_err(|e| {
            SshError::IoError(e)
        })?;

        // Read stderr
        let mut stderr_output = String::new();
        channel.stderr().read_to_string(&mut stderr_output).map_err(|e| {
            SshError::IoError(e)
        })?;

        // Check stderr for password prompts
        if stderr_output.contains("password is required") || stderr_output.contains("sudo:") {
            return Err(SshError::PermissionDenied(path.to_path_buf()));
        }

        // Check exit status
        channel.wait_close().ok();
        let exit_status = channel.exit_status().unwrap_or(-1);
        if exit_status != 0 {
            return Err(SshError::ConnectionFailed(format!(
                "sudo ls -la exited with status {}: {}",
                exit_status,
                stderr_output.trim()
            )));
        }

        Ok(parse_ls_la_output(&output, path))
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

/// Parse the output of `ls -la` into a vector of directory entries.
///
/// Skips "total NNN" header lines, "." and ".." entries, and lines with
/// fewer than 9 whitespace-separated fields or an unparseable size field.
/// For symlinks, strips the " -> target" suffix from the filename.
pub fn parse_ls_la_output(output: &str, parent_dir: &Path) -> Vec<DirectoryEntry> {
    let mut entries = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Skip "total NNN" header lines
        if line.starts_with("total ") {
            continue;
        }

        // Split into whitespace-separated fields; we need at least 9
        let fields: Vec<&str> = line.split_whitespace().collect();

        if fields.len() < 9 {
            continue;
        }

        // Field 0 first char determines entry type
        let entry_type = match fields[0].chars().next() {
            Some('d') => EntryType::Directory,
            Some('l') => EntryType::Symlink,
            _ => EntryType::File,
        };

        // Field 4 parsed as u64 for size; skip line if unparseable
        let size = match fields[4].parse::<u64>() {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Fields 8+ joined with spaces for filename (handles names with spaces)
        let raw_name = fields[8..].join(" ");

        // For symlinks, strip " -> target" suffix
        let name = if entry_type == EntryType::Symlink {
            if let Some(idx) = raw_name.find(" -> ") {
                raw_name[..idx].to_string()
            } else {
                raw_name
            }
        } else {
            raw_name
        };

        // Skip "." and ".." entries
        if name == "." || name == ".." {
            continue;
        }

        let path = parent_dir.join(&name);

        entries.push(DirectoryEntry {
            name,
            path,
            entry_type,
            size,
        });
    }

    entries
}

/// Simple shell escaping for a path used in a remote command.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Append a timestamped error line to `rfv-errors.log` in the current working directory.
/// Silently ignores write failures (best-effort logging).
fn log_error(context: &str, error: &SshError) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = timestamp.as_secs();

    // Manual UTC timestamp formatting: YYYY-MM-DDTHH:MM:SSZ
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate year, month, day from days since epoch (1970-01-01)
    let (year, month, day) = days_to_date(days);

    let ts = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    );

    let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("rfv-errors.log")
    else {
        return;
    };

    let _ = writeln!(file, "[{}] {}: {}", ts, context, error);
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_date(mut days: u64) -> (u64, u64, u64) {
    // Algorithm based on civil_from_days
    let mut year = 1970u64;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let leap = is_leap_year(year);
    let month_days: [u64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];

    let mut month = 1u64;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }

    (year, month, days + 1)
}

/// Check if a year is a leap year.
fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
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

    /// Format a slice of `DirectoryEntry` items into `ls -la` style output lines.
    ///
    /// Each line is formatted as:
    /// `{type_char}rw-r--r--  1 user group {size} Jan  1 00:00 {name}`
    ///
    /// For symlinks, ` -> /dev/null` is appended to exercise stripping logic.
    /// Lines are joined with `\n`.
    ///
    /// **Validates: Requirements 5.8**
    pub fn format_ls_la_output(entries: &[DirectoryEntry]) -> String {
        entries
            .iter()
            .map(|entry| {
                let type_char = match entry.entry_type {
                    EntryType::File => '-',
                    EntryType::Directory => 'd',
                    EntryType::Symlink => 'l',
                };

                let name_part = if entry.entry_type == EntryType::Symlink {
                    format!("{} -> /dev/null", entry.name)
                } else {
                    entry.name.clone()
                };

                format!(
                    "{}rw-r--r--  1 user group {} Jan  1 00:00 {}",
                    type_char, entry.size, name_part
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

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

    /// Strategy to generate a valid entry type.
    fn entry_type_strategy() -> impl Strategy<Value = EntryType> {
        prop_oneof![
            Just(EntryType::File),
            Just(EntryType::Directory),
            Just(EntryType::Symlink),
        ]
    }

    /// Strategy to generate a valid filename for ls -la round-trip testing.
    ///
    /// Constraints:
    /// - Non-empty, 1-30 chars
    /// - Printable ASCII (0x21-0x7E) with optional single spaces between non-space chars
    /// - No consecutive spaces (split_whitespace would collapse them)
    /// - Not "." or ".."
    /// - Must not start with "total " (those lines get skipped by the parser)
    /// - Regex: `[!-~]([!-~]| [!-~])*` gives printable non-space chars with single spaces between
    fn filename_strategy() -> impl Strategy<Value = String> {
        "[!-~]([!-~]| [!-~]){0,29}"
            .prop_filter("must not be . or ..", |s| s != "." && s != "..")
            .prop_filter("must not start with 'total '", |s| !s.starts_with("total "))
    }

    // Feature: sudo-fallback, Property 1: ls -la parsing round-trip
    // Validates: Requirements 1.2, 1.3, 1.4, 1.5, 5.1, 5.2, 5.3, 5.4, 5.5, 5.6, 5.8, 5.9
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn ls_la_parsing_round_trip(
            entries in proptest::collection::vec(
                (entry_type_strategy(), filename_strategy(), any::<u64>()),
                0..20
            )
        ) {
            let parent = Path::new("/test");

            // Build DirectoryEntry vec, applying symlink constraint
            let dir_entries: Vec<DirectoryEntry> = entries
                .into_iter()
                .filter(|(etype, name, _)| {
                    // For non-symlinks, name must not contain " -> "
                    if *etype != EntryType::Symlink {
                        !name.contains(" -> ")
                    } else {
                        true
                    }
                })
                .map(|(etype, name, size)| DirectoryEntry {
                    path: parent.join(&name),
                    name,
                    entry_type: etype,
                    size,
                })
                .collect();

            // Format then parse
            let formatted = format_ls_la_output(&dir_entries);
            let parsed = parse_ls_la_output(&formatted, parent);

            // Assert parsed entries match originals
            prop_assert_eq!(parsed.len(), dir_entries.len(), "entry count mismatch");
            for (original, parsed_entry) in dir_entries.iter().zip(parsed.iter()) {
                prop_assert_eq!(&parsed_entry.name, &original.name, "name mismatch");
                prop_assert_eq!(&parsed_entry.entry_type, &original.entry_type, "entry_type mismatch");
                prop_assert_eq!(parsed_entry.size, original.size, "size mismatch");
                prop_assert_eq!(&parsed_entry.path, &original.path, "path mismatch");
            }
        }
    }

    /// Simulate the chunked reading logic used by `sudo_cat`.
    ///
    /// Reads input bytes in 8 KiB chunks from a Cursor reader, writes to a Cursor writer,
    /// and returns the total byte count.
    fn simulate_chunked_copy(input: &[u8]) -> u64 {
        use std::io::{Cursor, Read, Write};
        let mut reader = Cursor::new(input);
        let mut writer = Cursor::new(Vec::new());
        let mut buf = [0u8; 8192];
        let mut total: u64 = 0;
        loop {
            let n = reader.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            writer.write_all(&buf[..n]).unwrap();
            total += n as u64;
        }
        total
    }

    // Feature: sudo-fallback, Property 2: sudo_cat byte count correctness
    // Validates: Requirements 2.2
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn sudo_cat_byte_count_correctness(data in proptest::collection::vec(any::<u8>(), 0..65536)) {
            let byte_count = simulate_chunked_copy(&data);
            prop_assert_eq!(byte_count, data.len() as u64);
        }
    }
}
