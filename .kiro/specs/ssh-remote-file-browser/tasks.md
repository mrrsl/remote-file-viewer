# Implementation Plan: SSH Remote File Browser

## Overview

Build a Rust TUI application that connects to a remote server via SSH, browses its file system interactively, and supports file viewing, copying, and searching. The implementation follows the module structure defined in the design: config parsing → SSH client → app state machine → event loop → UI rendering → operations. Each task builds incrementally, starting with foundational types and config, layering in SSH connectivity, then the interactive TUI with navigation, file operations, and search.

## Tasks

- [x] 1. Set up project structure and shared types
  - [x] 1.1 Add dependencies and create module skeleton
    - Add `ssh2`, `proptest` (dev), and `tempfile` (dev) to Cargo.toml
    - Create the module file structure: `src/config.rs`, `src/ssh.rs`, `src/app.rs`, `src/event.rs`, `src/types.rs`, `src/ui/mod.rs`, `src/ui/header.rs`, `src/ui/browser.rs`, `src/ui/footer.rs`, `src/ui/search_prompt.rs`, `src/ui/path_prompt.rs`, `src/ui/navigate_prompt.rs`, `src/operations/mod.rs`, `src/operations/listing.rs`, `src/operations/download.rs`, `src/operations/search.rs`, `src/operations/navigate.rs`
    - Declare all modules in `src/main.rs` with placeholder contents
    - _Requirements: 1.1, 1.4_

  - [x] 1.2 Implement shared types module (`src/types.rs`)
    - Define `DirectoryEntry` struct with `name`, `path`, `entry_type`, `size` fields
    - Define `EntryType` enum (File, Directory, Symlink)
    - Define `StatusMessage` struct with `text`, `level`, `created_at` fields
    - Define `StatusLevel` enum (Info, Success, Error)
    - Implement `format_size(bytes: u64) -> String` for human-readable sizes (B, KB, MB, GB)
    - Implement `truncate_name(name: &str, max_len: usize) -> String` with "..." suffix for overflow
    - _Requirements: 2.2_

  - [x] 1.3 Write property tests for shared types
    - **Property 3: Entry display formatting**
    - **Validates: Requirements 2.2**
    - Test `truncate_name` with arbitrary strings: result ≤ max_len+3, ends with "..." when truncated, equals original when not
    - Test `format_size` with arbitrary u64: result is non-empty, matches `<number> <unit>` pattern

- [x] 2. Implement config parsing
  - [x] 2.1 Implement config parser (`src/config.rs`)
    - Define `AppConfig` struct with `ssh_identity_file: PathBuf`, `username: String`, `ip_address: String`
    - Define `ConfigError` enum (FileNotFound, ReadError, MissingField, ParseError)
    - Implement `AppConfig::from_file(path: &Path) -> Result<Self, ConfigError>`
    - Handle comments (lines starting with '#'), whitespace trimming, key=value parsing
    - Validate all three required fields are present
    - Implement `Display` for `ConfigError` with user-friendly messages
    - _Requirements: 1.1, 1.2, 1.3, 1.5, 1.6_

  - [x] 2.2 Write property tests for config parsing
    - **Property 1: Config parsing round trip**
    - **Validates: Requirements 1.3**
    - Generate valid config content with arbitrary non-empty values and optional whitespace/comments, verify parsing succeeds and returns exact values

  - [x] 2.3 Write property test for missing field detection
    - **Property 2: Config missing field detection**
    - **Validates: Requirements 1.6**
    - Generate config content with 1-3 fields randomly removed, verify `ConfigError::MissingField` is returned identifying an absent field

- [x] 3. Implement SSH client
  - [x] 3.1 Implement SSH session and SFTP wrapper (`src/ssh.rs`)
    - Define `SshClient` struct holding `ssh2::Session` and `ssh2::Sftp`
    - Define `SshError` enum (ConnectionFailed, AuthenticationFailed, Timeout, PermissionDenied, ConnectionLost, IoError)
    - Implement `SshClient::connect(config: &AppConfig) -> Result<Self, SshError>` with 30-second timeout
    - Implement `list_dir(&self, path: &Path) -> Result<Vec<DirectoryEntry>, SshError>`
    - Implement `download_file(&self, remote_path: &Path, writer: &mut impl Write) -> Result<u64, SshError>`
    - Implement `stat(&self, path: &Path) -> Result<FileStat, SshError>`
    - Implement `find_recursive(&self, base: &Path, pattern: &str) -> Result<Vec<DirectoryEntry>, SshError>` using remote `find` command execution
    - Implement `is_connected(&self) -> bool`
    - Implement `Display` for `SshError` with user-friendly messages
    - _Requirements: 1.4, 1.7, 1.8, 2.5, 2.6_

- [x] 4. Implement application state machine
  - [x] 4.1 Implement app state (`src/app.rs`)
    - Define `AppMode` enum (Browsing, SearchPrompt, Searching, SearchResults, PathPrompt, OverwriteConfirm, Copying, NavigatePrompt)
    - Define `SearchType` enum (Local, Global)
    - Define `SearchState`, `PathPromptState`, `NavigatePromptState` structs
    - Define `App` struct with mode, current_path, entries, selected_index, show_hidden, status_message, search/prompt states, loading flag
    - Implement `App::new(initial_path, entries)`
    - Implement `move_cursor_up()`, `move_cursor_down()` with bounds clamping
    - Implement `selected_entry()`, `set_status()`, `clear_expired_status()`
    - Implement `select_entry_by_name(&mut self, name: &str)` for cursor placement
    - _Requirements: 3.1, 3.2, 3.3, 3.4, 6.2_

  - [x] 4.2 Write property tests for cursor movement
    - **Property 5: Cursor movement bounds**
    - **Validates: Requirements 3.1, 3.2, 3.3, 3.4**
    - For arbitrary list length N>0 and cursor position i in [0, N-1]: verify move_down/move_up clamp correctly

  - [x] 4.3 Write property test for status message visibility
    - **Property 8: Status message visibility**
    - **Validates: Requirements 6.2**
    - For arbitrary durations, verify message is visible (not clearable) when elapsed < 3s and clearable when >= 3s

  - [x] 4.4 Write property test for cursor placement by name
    - **Property 12: Navigate cursor placement on file target**
    - **Validates: Requirements 8.5**
    - For arbitrary entry lists containing a target filename, verify `select_entry_by_name` sets cursor to the correct index (or 0 if not found)

- [x] 5. Checkpoint
  - Ensure all tests pass, ask the user if questions arise.

- [x] 6. Implement directory operations
  - [x] 6.1 Implement directory listing logic (`src/operations/listing.rs`)
    - Implement `list_directory(ssh, path, show_hidden) -> Result<Vec<DirectoryEntry>, SshError>` that calls `ssh.list_dir`, filters hidden entries, and sorts
    - Implement `sort_entries(entries: &mut Vec<DirectoryEntry>)` — directories first, then case-insensitive alphabetical within each group
    - _Requirements: 2.1, 2.3, 2.7_

  - [x] 6.2 Write property test for entry sorting
    - **Property 4: Entry sorting invariant**
    - **Validates: Requirements 2.3**
    - For arbitrary Vec of DirectoryEntry items, verify all directories appear before files, and within each group entries are case-insensitively alphabetical

  - [x] 6.3 Implement path navigation helpers
    - Implement parent path computation (parent of root is root itself)
    - Implement relative path resolution against working directory
    - _Requirements: 3.7, 3.8, 5.3_

  - [x] 6.4 Write property tests for path navigation
    - **Property 6: Parent path navigation**
    - **Validates: Requirements 3.7, 3.8**
    - For arbitrary absolute paths: parent of non-root removes last component, parent of root is root
    - **Property 7: Relative path resolution**
    - **Validates: Requirements 5.3**
    - For arbitrary relative path and working directory: resolved path starts with working directory

- [x] 7. Implement search operations
  - [x] 7.1 Implement search logic (`src/operations/search.rs`)
    - Implement `local_find(ssh, dir, query, show_hidden) -> Result<Vec<DirectoryEntry>, SshError>` — non-recursive case-insensitive substring match
    - Implement `global_find(ssh, base, query, show_hidden, on_progress) -> Result<Vec<DirectoryEntry>, SshError>` — recursive with abort callback
    - The `on_progress` callback reports directory count and returns `false` to abort
    - _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5, 7.7, 7.11_

  - [x] 7.2 Write property test for search matching
    - **Property 9: Case-insensitive substring matching**
    - **Validates: Requirements 7.4, 7.5**
    - For arbitrary name and query strings: match predicate returns true iff lowercased name contains lowercased query

  - [x] 7.3 Write property test for search result display
    - **Property 10: Search result display completeness**
    - **Validates: Requirements 7.6**
    - For arbitrary DirectoryEntry with base path: formatted result contains relative path, valid size string, and type indicator

- [x] 8. Implement direct path navigation
  - [x] 8.1 Implement navigate logic (`src/operations/navigate.rs`)
    - Implement `validate_absolute_path(input: &str) -> bool` — true iff starts with '/'
    - Implement `resolve_navigate_target(ssh, path) -> Result<NavigateTarget, SshError>` — stat the path, return Directory or File variant
    - Define `NavigateTarget` enum (Directory(PathBuf), File { parent, filename })
    - _Requirements: 8.1, 8.2, 8.3, 8.4, 8.5, 8.6_

  - [x] 8.2 Write property test for absolute path validation
    - **Property 11: Absolute path validation**
    - **Validates: Requirements 8.2**
    - For arbitrary non-empty strings: returns true iff string starts with '/'

- [x] 9. Checkpoint
  - Ensure all tests pass, ask the user if questions arise.

- [x] 10. Implement UI rendering
  - [x] 10.1 Implement header widget (`src/ui/header.rs`)
    - Render current absolute path and connection info (username@host) in a styled bar
    - _Requirements: 3.9, 6.1_

  - [x] 10.2 Implement file browser widget (`src/ui/browser.rs`)
    - Render list of DirectoryEntry items with selection highlighting
    - Display name (truncated), size, and type indicator for each entry
    - Show empty-directory message when entries list is empty
    - Handle scrolling for long lists (viewport follows cursor)
    - _Requirements: 2.1, 2.2, 2.4_

  - [x] 10.3 Implement footer widget (`src/ui/footer.rs`)
    - Render key bindings in default state (q: quit, o: open, c: copy, Ctrl+F: find, Ctrl+G: global find, m: navigate, a: toggle hidden)
    - Render status messages (info/success/error) when active, replacing key bindings
    - Render loading indicator during operations
    - _Requirements: 6.1, 6.2, 6.3_

  - [x] 10.4 Implement search prompt widget (`src/ui/search_prompt.rs`)
    - Render text input field with label indicating Local or Global find
    - Show progress indicator (directories traversed) during active Global_Find
    - _Requirements: 7.1, 7.2, 7.11_

  - [x] 10.5 Implement path prompt widget (`src/ui/path_prompt.rs`)
    - Render text input field pre-filled with remote filename as default
    - Handle overwrite confirmation display
    - Show transfer progress (bytes transferred) during copy
    - _Requirements: 5.1, 5.4, 5.6, 5.11_

  - [x] 10.6 Implement navigate prompt widget (`src/ui/navigate_prompt.rs`)
    - Render text input field for absolute remote path entry
    - _Requirements: 8.1_

  - [x] 10.7 Implement top-level UI render function (`src/ui/mod.rs`)
    - Compose layout with header, main area, and footer regions
    - Dispatch to appropriate widget based on current AppMode
    - Render Search_Results view with relative paths, sizes, and type indicators
    - Show no-results message when search returns empty
    - _Requirements: 6.1, 7.6, 7.12_

- [x] 11. Implement event handling and operations
  - [x] 11.1 Implement keyboard event dispatch (`src/event.rs`)
    - Map key events to actions based on current AppMode
    - Browsing mode: arrows/j/k (move), Enter (open dir), Backspace/'-' (parent), 'o' (view), 'c' (copy), 'q' (quit), 'a' (toggle hidden), Ctrl+F (local find), Ctrl+G (global find), 'm' (navigate)
    - SearchPrompt mode: character input, Enter (confirm), Escape (cancel)
    - SearchResults mode: arrows/j/k (move), Enter (select result), Escape (close)
    - PathPrompt mode: character input, Enter (confirm), Escape (cancel)
    - OverwriteConfirm mode: 'y' (confirm), 'n'/Escape (cancel)
    - NavigatePrompt mode: character input, Enter (confirm), Escape (cancel)
    - Searching mode: Escape (abort)
    - _Requirements: 3.1–3.11, 5.10, 7.7, 7.8, 7.9, 7.10, 7.13, 8.7_

  - [x] 11.2 Implement file view operation (`src/operations/download.rs`)
    - Download remote file to temp file using `tempfile` crate
    - Enforce 50 MB size limit check before download
    - Determine editor from EDITOR → VISUAL → "less"
    - Suspend TUI (leave alternate screen), spawn editor process, wait for exit
    - Resume TUI (re-enter alternate screen), delete temp file
    - Handle errors: permission denied, size exceeded, editor launch failure, download failure with partial cleanup
    - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5, 4.6, 4.7, 4.8_

  - [x] 11.3 Implement file copy operation
    - Trigger Path_Prompt with pre-filled remote filename
    - Resolve user-entered relative path against launch working directory
    - Check parent directory exists, check if destination file exists (trigger overwrite confirm)
    - Stream file download to local path, report progress (bytes transferred)
    - On success: display success status with resolved path
    - On error: delete partial file, display error in status bar
    - _Requirements: 5.1, 5.2, 5.3, 5.4, 5.5, 5.6, 5.7, 5.8, 5.9, 5.10, 5.11_

  - [x] 11.4 Wire search operations into event loop
    - Connect Ctrl+F → local_find, Ctrl+G → global_find
    - Pass on_progress callback for global find that checks for Escape abort
    - Display Search_Results, handle Enter to navigate to selected result
    - Navigate to file's parent directory with cursor on file, or into selected directory
    - _Requirements: 7.1–7.13_

  - [x] 11.5 Wire navigate operation into event loop
    - Connect 'm' → NavigatePrompt
    - On Enter: validate absolute path, resolve target via SSH stat
    - If directory: navigate and list contents
    - If file: navigate to parent, set cursor on file
    - If error: display in status bar, remain on current directory
    - On Escape: cancel and return to Browsing
    - _Requirements: 8.1–8.7_

- [x] 12. Implement main entry point and terminal lifecycle
  - [x] 12.1 Wire everything together in `src/main.rs`
    - Parse command-line args (optional config path, default `./kafka-term-config`)
    - Parse config, handle errors (print to stderr, exit 1)
    - Establish SSH connection, handle errors (print to stderr, exit 1)
    - List home directory as initial content
    - Set up panic hook to restore terminal state
    - Enter alternate screen, enable raw mode
    - Run event loop: poll events → dispatch → update state → render
    - Handle connection-lost: restore terminal, print error, exit 1
    - On quit: close SSH, restore terminal, exit 0
    - _Requirements: 1.1, 1.2, 1.4, 1.5, 1.7, 1.8, 2.1, 3.10, 6.4_

- [x] 13. Final checkpoint
  - Ensure all tests pass, ask the user if questions arise.

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- Property tests validate universal correctness properties from the design document using `proptest` crate
- Unit tests validate specific examples and edge cases
- The SSH client (task 3.1) cannot be easily unit tested without a real SSH server; integration testing requires a Docker sshd container
- Terminal lifecycle safety (panic hook, Drop guard) is critical and handled in task 12.1

## Task Dependency Graph

```json
{
  "waves": [
    { "id": 0, "tasks": ["1.1"] },
    { "id": 1, "tasks": ["1.2"] },
    { "id": 2, "tasks": ["1.3", "2.1", "3.1"] },
    { "id": 3, "tasks": ["2.2", "2.3", "4.1"] },
    { "id": 4, "tasks": ["4.2", "4.3", "4.4", "6.1"] },
    { "id": 5, "tasks": ["6.2", "6.3", "7.1", "8.1"] },
    { "id": 6, "tasks": ["6.4", "7.2", "7.3", "8.2"] },
    { "id": 7, "tasks": ["10.1", "10.2", "10.3", "10.4", "10.5", "10.6"] },
    { "id": 8, "tasks": ["10.7", "11.1"] },
    { "id": 9, "tasks": ["11.2", "11.3", "11.4", "11.5"] },
    { "id": 10, "tasks": ["12.1"] }
  ]
}
```
