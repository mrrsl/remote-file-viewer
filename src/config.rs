// Config file parsing for SSH connection parameters

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Application configuration holding SSH connection parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct AppConfig {
    pub ssh_identity_file: PathBuf,
    pub username: String,
    pub ip_address: String,
}

/// Errors that can occur during config file parsing.
#[derive(Debug)]
pub enum ConfigError {
    FileNotFound(PathBuf),
    ReadError(PathBuf, io::Error),
    MissingField(&'static str),
    ParseError(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::FileNotFound(path) => {
                write!(f, "Config file not found: {}", path.display())
            }
            ConfigError::ReadError(path, err) => {
                write!(f, "Failed to read config file '{}': {}", path.display(), err)
            }
            ConfigError::MissingField(field) => {
                write!(f, "Missing required field '{}' in config file", field)
            }
            ConfigError::ParseError(msg) => {
                write!(f, "Config parse error: {}", msg)
            }
        }
    }
}

impl AppConfig {
    /// Parse config from the given file path. Returns detailed error on failure.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Err(ConfigError::FileNotFound(path.to_path_buf()));
        }

        let content = fs::read_to_string(path)
            .map_err(|e| ConfigError::ReadError(path.to_path_buf(), e))?;

        Self::parse_content(&content)
    }

    /// Parse config from a string. Useful for testing without files.
    pub fn parse_content(content: &str) -> Result<Self, ConfigError> {
        let mut ssh_identity_file: Option<PathBuf> = None;
        let mut username: Option<String> = None;
        let mut ip_address: Option<String> = None;

        for line in content.lines() {
            let trimmed = line.trim();

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Parse key=value
            let Some((key, value)) = trimmed.split_once('=') else {
                return Err(ConfigError::ParseError(format!(
                    "Invalid line (expected key=value): '{}'",
                    trimmed
                )));
            };

            let key = key.trim();
            let value = value.trim();

            match key {
                "ssh_identity_file" => {
                    ssh_identity_file = Some(PathBuf::from(value));
                }
                "username" => {
                    username = Some(value.to_string());
                }
                "ip_address" => {
                    ip_address = Some(value.to_string());
                }
                _ => {
                    // Ignore unknown keys
                }
            }
        }

        let ssh_identity_file =
            ssh_identity_file.ok_or(ConfigError::MissingField("ssh_identity_file"))?;
        let username = username.ok_or(ConfigError::MissingField("username"))?;
        let ip_address = ip_address.ok_or(ConfigError::MissingField("ip_address"))?;

        Ok(AppConfig {
            ssh_identity_file,
            username,
            ip_address,
        })
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_valid_config() {
        let content = "\
ssh_identity_file=/home/user/.ssh/id_rsa
username=deploy
ip_address=192.168.1.100
";
        let config = AppConfig::parse_content(content).unwrap();
        assert_eq!(
            config.ssh_identity_file,
            PathBuf::from("/home/user/.ssh/id_rsa")
        );
        assert_eq!(config.username, "deploy");
        assert_eq!(config.ip_address, "192.168.1.100");
    }

    #[test]
    fn test_parse_with_comments_and_whitespace() {
        let content = "\
# SSH Configuration
  ssh_identity_file = /home/user/.ssh/id_rsa  
  username = deploy  

# IP of the server
  ip_address = 10.0.0.1  
";
        let config = AppConfig::parse_content(content).unwrap();
        assert_eq!(
            config.ssh_identity_file,
            PathBuf::from("/home/user/.ssh/id_rsa")
        );
        assert_eq!(config.username, "deploy");
        assert_eq!(config.ip_address, "10.0.0.1");
    }

    #[test]
    fn test_parse_missing_username() {
        let content = "\
ssh_identity_file=/home/user/.ssh/id_rsa
ip_address=192.168.1.100
";
        let err = AppConfig::parse_content(content).unwrap_err();
        match err {
            ConfigError::MissingField(field) => assert_eq!(field, "username"),
            _ => panic!("Expected MissingField error, got: {:?}", err),
        }
    }

    #[test]
    fn test_parse_missing_ssh_identity_file() {
        let content = "\
username=deploy
ip_address=192.168.1.100
";
        let err = AppConfig::parse_content(content).unwrap_err();
        match err {
            ConfigError::MissingField(field) => assert_eq!(field, "ssh_identity_file"),
            _ => panic!("Expected MissingField error, got: {:?}", err),
        }
    }

    #[test]
    fn test_parse_missing_ip_address() {
        let content = "\
ssh_identity_file=/home/user/.ssh/id_rsa
username=deploy
";
        let err = AppConfig::parse_content(content).unwrap_err();
        match err {
            ConfigError::MissingField(field) => assert_eq!(field, "ip_address"),
            _ => panic!("Expected MissingField error, got: {:?}", err),
        }
    }

    #[test]
    fn test_parse_invalid_line() {
        let content = "\
ssh_identity_file=/home/user/.ssh/id_rsa
username=deploy
this-is-not-valid
ip_address=192.168.1.100
";
        let err = AppConfig::parse_content(content).unwrap_err();
        match err {
            ConfigError::ParseError(msg) => {
                assert!(msg.contains("this-is-not-valid"));
            }
            _ => panic!("Expected ParseError, got: {:?}", err),
        }
    }

    #[test]
    fn test_from_file_not_found() {
        let path = Path::new("/nonexistent/path/config");
        let err = AppConfig::from_file(path).unwrap_err();
        match err {
            ConfigError::FileNotFound(p) => assert_eq!(p, path),
            _ => panic!("Expected FileNotFound error, got: {:?}", err),
        }
    }

    #[test]
    fn test_from_file_valid() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        writeln!(tmpfile, "ssh_identity_file=/tmp/key").unwrap();
        writeln!(tmpfile, "username=testuser").unwrap();
        writeln!(tmpfile, "ip_address=127.0.0.1").unwrap();

        let config = AppConfig::from_file(tmpfile.path()).unwrap();
        assert_eq!(config.ssh_identity_file, PathBuf::from("/tmp/key"));
        assert_eq!(config.username, "testuser");
        assert_eq!(config.ip_address, "127.0.0.1");
    }

    #[test]
    fn test_display_file_not_found() {
        let err = ConfigError::FileNotFound(PathBuf::from("/some/path"));
        assert_eq!(err.to_string(), "Config file not found: /some/path");
    }

    #[test]
    fn test_display_missing_field() {
        let err = ConfigError::MissingField("username");
        assert_eq!(
            err.to_string(),
            "Missing required field 'username' in config file"
        );
    }

    #[test]
    fn test_display_parse_error() {
        let err = ConfigError::ParseError("bad line".to_string());
        assert_eq!(err.to_string(), "Config parse error: bad line");
    }

    #[test]
    fn test_parse_empty_content() {
        let err = AppConfig::parse_content("").unwrap_err();
        match err {
            ConfigError::MissingField(_) => {}
            _ => panic!("Expected MissingField error for empty config, got: {:?}", err),
        }
    }

    #[test]
    fn test_parse_only_comments() {
        let content = "\
# This is a comment
# Another comment
";
        let err = AppConfig::parse_content(content).unwrap_err();
        match err {
            ConfigError::MissingField(_) => {}
            _ => panic!(
                "Expected MissingField error for comment-only config, got: {:?}",
                err
            ),
        }
    }

    #[test]
    fn test_parse_ignores_unknown_keys() {
        let content = "\
ssh_identity_file=/home/user/.ssh/id_rsa
username=deploy
ip_address=192.168.1.100
extra_key=some_value
";
        let config = AppConfig::parse_content(content).unwrap();
        assert_eq!(config.username, "deploy");
    }

    #[test]
    fn test_parse_value_with_equals_sign() {
        let content = "\
ssh_identity_file=/home/user/.ssh/id_rsa
username=deploy
ip_address=192.168.1.100=extra
";
        let config = AppConfig::parse_content(content).unwrap();
        // split_once('=') means the value includes everything after the first '='
        assert_eq!(config.ip_address, "192.168.1.100=extra");
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    /// Generate a non-empty string that doesn't contain newlines, '=' characters,
    /// or leading/trailing whitespace (since the parser trims values).
    fn valid_value_strategy() -> impl Strategy<Value = String> {
        "[^\n\r\t =][^\n\r=]*[^\n\r\t =]"
            .prop_filter("must not be empty after trim", |s| !s.trim().is_empty())
            .prop_map(|s| s.trim().to_string())
    }

    /// Generate an optional comment line (starting with '#') with no newlines.
    fn comment_line_strategy() -> impl Strategy<Value = String> {
        "[^\n\r]{0,40}".prop_map(|s| format!("# {}", s))
    }

    /// Generate optional whitespace (spaces/tabs, no newlines).
    fn opt_whitespace() -> impl Strategy<Value = String> {
        "[ \t]{0,4}"
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        // Feature: ssh-remote-file-browser, Property 1: Config parsing round trip
        /// **Validates: Requirements 1.3**
        /// For any valid config content with arbitrary non-empty values and optional
        /// whitespace/comments, parsing should succeed and return exact values.
        #[test]
        fn config_parsing_round_trip(
            ssh_identity_file in valid_value_strategy(),
            username in valid_value_strategy(),
            ip_address in valid_value_strategy(),
            comment1 in proptest::option::of(comment_line_strategy()),
            comment2 in proptest::option::of(comment_line_strategy()),
            ws_before_key in opt_whitespace(),
            ws_after_key in opt_whitespace(),
            ws_before_val in opt_whitespace(),
            ws_after_val in opt_whitespace(),
        ) {
            // Build config content with optional comments and whitespace
            let mut content = String::new();

            if let Some(c) = &comment1 {
                content.push_str(c);
                content.push('\n');
            }

            // ssh_identity_file line with optional surrounding whitespace
            content.push_str(&format!(
                "{}ssh_identity_file{}={}{}{}\n",
                ws_before_key, ws_after_key, ws_before_val, ssh_identity_file, ws_after_val
            ));

            if let Some(c) = &comment2 {
                content.push_str(c);
                content.push('\n');
            }

            // username line
            content.push_str(&format!(
                "{}username{}={}{}{}\n",
                ws_before_key, ws_after_key, ws_before_val, username, ws_after_val
            ));

            // ip_address line
            content.push_str(&format!(
                "{}ip_address{}={}{}{}\n",
                ws_before_key, ws_after_key, ws_before_val, ip_address, ws_after_val
            ));

            let result = AppConfig::parse_content(&content);
            prop_assert!(result.is_ok(), "Parsing failed for content:\n{}\nError: {:?}", content, result.err());

            let config = result.unwrap();
            prop_assert_eq!(
                config.ssh_identity_file.to_str().unwrap(),
                ssh_identity_file.as_str(),
                "ssh_identity_file mismatch"
            );
            prop_assert_eq!(&config.username, &username, "username mismatch");
            prop_assert_eq!(&config.ip_address, &ip_address, "ip_address mismatch");
        }

        // Feature: ssh-remote-file-browser, Property 2: Config missing field detection
        // Validates: Requirements 1.6
        #[test]
        fn missing_field_returns_error_identifying_absent_field(
            ssh_val in "[^\n=]{1,50}",
            user_val in "[^\n=]{1,50}",
            ip_val in "[^\n=]{1,50}",
            include_ssh in proptest::bool::ANY,
            include_user in proptest::bool::ANY,
            include_ip in proptest::bool::ANY,
        ) {
            // Ensure at least one field is missing (never include all 3)
            prop_assume!(!(include_ssh && include_user && include_ip));
            // Ensure at least one field is included (otherwise it's trivially all missing)
            prop_assume!(include_ssh || include_user || include_ip);

            let mut content = String::new();
            if include_ssh {
                content.push_str(&format!("ssh_identity_file={}\n", ssh_val));
            }
            if include_user {
                content.push_str(&format!("username={}\n", user_val));
            }
            if include_ip {
                content.push_str(&format!("ip_address={}\n", ip_val));
            }

            let result = AppConfig::parse_content(&content);
            prop_assert!(result.is_err(), "Expected Err when field(s) are missing, got Ok");

            match result.unwrap_err() {
                ConfigError::MissingField(field_name) => {
                    // The reported missing field must actually be absent from the content
                    let absent_fields: Vec<&str> = [
                        (!include_ssh).then_some("ssh_identity_file"),
                        (!include_user).then_some("username"),
                        (!include_ip).then_some("ip_address"),
                    ]
                    .iter()
                    .filter_map(|f| *f)
                    .collect();

                    prop_assert!(
                        absent_fields.contains(&field_name),
                        "MissingField({:?}) reported but absent fields are {:?}",
                        field_name,
                        absent_fields
                    );
                }
                other => {
                    prop_assert!(false, "Expected ConfigError::MissingField, got: {:?}", other);
                }
            }
        }
    }
}
