# Requirements Document

## Introduction

A Rust command-line TUI application built with ratatui that enables users to browse remote server file systems over SSH. The application addresses the workflow pain point of manually SSHing into remote Kafka servers, navigating to find log files, and then having to exit the session and use separate commands to copy files locally. Instead, this tool provides a single interactive interface for navigating, viewing, and copying remote files.

## Glossary

- **TUI**: Terminal User Interface — the interactive text-based interface rendered in the terminal
- **Application**: The ssh-remote-file-browser Rust binary
- **File_Browser**: The main TUI component that displays the remote file system directory listing
- **SSH_Client**: The component responsible for establishing and maintaining the SSH connection to the remote server using identity-file-based authentication
- **Remote_Server**: The target machine connected to via SSH
- **Config_File**: The configuration file (default: `./kafka-term-config`) containing ssh_identity_file, username, and ip_address for the SSH connection
- **File_Viewer**: The component that opens a remote file's contents in the user's preferred local editor
- **File_Copier**: The component responsible for transferring a remote file to the local machine
- **Path_Prompt**: The TUI input widget that prompts the user to type a local destination path for file copy operations
- **Directory_Entry**: A single item (file or directory) displayed in the File_Browser listing
- **Preferred_Editor**: The local text editor determined by the EDITOR or VISUAL environment variable
- **Local_Find**: A search operation that matches file and directory names within the currently displayed directory only (non-recursive)
- **Global_Find**: A recursive search operation that matches file and directory names starting from the current directory and traversing all subdirectories
- **Search_Prompt**: The TUI input widget that accepts a search query string from the user
- **Search_Results**: The TUI view that displays the list of Directory_Entry items matching the search query
- **Navigate_Prompt**: The TUI input widget that prompts the user to type a remote path for direct navigation

## Requirements

### Requirement 1: SSH Connection via Config File

**User Story:** As a user, I want to connect to a remote server via SSH using connection details from a config file, so that I can interact with its file system without manually typing connection parameters each time.

#### Acceptance Criteria

1. WHEN the Application is launched without arguments, THE Application SHALL read SSH connection parameters (ssh_identity_file, username, ip_address) from a file named `./kafka-term-config` in the current working directory.
2. WHEN the Application is launched with a single command-line argument, THE Application SHALL treat that argument as the path to the config file and read SSH connection parameters from it.
3. THE config file SHALL contain three values: `ssh_identity_file` (path to the private key file), `username` (SSH login user), and `ip_address` (the remote host address).
4. WHEN the config file is successfully parsed, THE SSH_Client SHALL establish an SSH connection to the Remote_Server using the identity file for authentication (equivalent to `ssh -i <ssh_identity_file> <username>@<ip_address>`) within 30 seconds.
5. IF the config file does not exist or cannot be read, THEN THE Application SHALL display an error message indicating the missing or unreadable config file path and exit with a non-zero status code.
6. IF the config file is missing any required field (ssh_identity_file, username, or ip_address), THEN THE Application SHALL display an error message indicating which field is missing and exit with a non-zero status code.
7. IF the SSH connection fails, THEN THE Application SHALL display an error message indicating the failure reason (e.g., host unreachable, authentication failure, invalid identity file, or connection timeout) and exit with a non-zero status code.
8. IF the SSH connection is lost during operation, THEN THE Application SHALL restore the terminal to its original state, display a connection-lost message to stderr, and exit with a non-zero status code.

### Requirement 2: Remote Directory Listing

**User Story:** As a user, I want to see the contents of directories on the remote server, so that I can find the files I need.

#### Acceptance Criteria

1. WHEN the SSH connection is established, THE File_Browser SHALL display the contents of the user's home directory on the Remote_Server, excluding hidden entries (names starting with '.') by default.
2. THE File_Browser SHALL display each Directory_Entry with its name (truncated to 60 characters with an ellipsis if longer), size in human-readable format (B, KB, MB, GB), and type indicator (file, directory, or symlink).
3. THE File_Browser SHALL sort Directory_Entry items with directories listed before files, and case-insensitively in alphabetical order within each group.
4. WHEN a directory contains no readable entries, THE File_Browser SHALL display an empty directory message in the main area.
5. IF a directory listing command fails due to permissions, THEN THE File_Browser SHALL display a permission-denied error message in the TUI status bar and remain on the previously listed directory.
6. IF the directory listing response is not received within 10 seconds, THEN THE File_Browser SHALL display a timeout error message in the TUI status bar and remain on the previously listed directory.
7. WHEN the user presses 'a', THE File_Browser SHALL toggle visibility of hidden entries and refresh the current directory listing.

### Requirement 3: File System Navigation

**User Story:** As a user, I want to navigate through the remote file system using keyboard controls, so that I can move between directories efficiently.

#### Acceptance Criteria

1. WHEN the user presses the down-arrow or 'j' key, THE File_Browser SHALL move the selection cursor to the next Directory_Entry in the list.
2. WHEN the user presses the up-arrow or 'k' key, THE File_Browser SHALL move the selection cursor to the previous Directory_Entry in the list.
3. WHEN the selection cursor is on the last Directory_Entry and the user presses down-arrow or 'j', THE File_Browser SHALL keep the cursor on the last entry without wrapping.
4. WHEN the selection cursor is on the first Directory_Entry and the user presses up-arrow or 'k', THE File_Browser SHALL keep the cursor on the first entry without wrapping.
5. WHEN the user presses Enter on a selected directory, THE File_Browser SHALL navigate into that directory and display its contents with the cursor on the first entry.
6. WHEN the user presses Enter on a selected file, THE File_Browser SHALL take no action.
7. WHEN the user presses Backspace or '-' key, THE File_Browser SHALL navigate to the parent directory and display its contents.
8. WHEN the current directory is the file system root ('/') and the user presses Backspace or '-', THE File_Browser SHALL remain on the root directory without navigating.
9. WHILE the File_Browser is displaying a directory listing, THE File_Browser SHALL show the current absolute path in the header bar.
10. WHEN the user presses 'q', THE Application SHALL close the SSH connection and exit.
11. WHEN the File_Browser navigates into a new directory, THE selection cursor SHALL be placed on the first Directory_Entry.

### Requirement 4: View Remote File Contents

**User Story:** As a user, I want to view a remote file's contents in my preferred local editor, so that I can read log files without manually copying them first.

#### Acceptance Criteria

1. WHEN the user presses 'o' with a file selected, THE File_Viewer SHALL download the file contents to a temporary local file and open the Preferred_Editor with that temporary file.
2. THE File_Viewer SHALL determine the Preferred_Editor by checking the EDITOR environment variable first, then the VISUAL environment variable, and falling back to "less" if neither is set.
3. WHILE the Preferred_Editor is open, THE Application SHALL suspend the TUI rendering and yield the terminal to the editor process.
4. WHEN the Preferred_Editor process exits, THE Application SHALL resume TUI rendering and delete the temporary file.
5. IF the remote file cannot be read due to permissions or exceeds 50 MB in size, THEN THE File_Viewer SHALL display an error message in the TUI status bar indicating the reason for failure without opening an editor.
6. IF the Preferred_Editor process cannot be launched, THEN THE File_Viewer SHALL delete the temporary file and display an error message in the TUI status bar indicating the editor could not be started.
7. WHEN the user presses 'o' with a directory selected, THE File_Viewer SHALL take no action.
8. IF the file download fails before completion, THEN THE File_Viewer SHALL delete any partially written temporary file and display an error message in the TUI status bar indicating the download failure.

### Requirement 5: Copy Remote File to Local Machine

**User Story:** As a user, I want to copy a remote file to my local machine by specifying a local path, so that I can save log files locally without leaving the application.

#### Acceptance Criteria

1. WHEN the user presses 'c' with a file selected, THE Application SHALL display the Path_Prompt asking for a local destination path.
2. WHEN the user presses 'c' with a directory selected, THE Application SHALL take no action.
3. THE Path_Prompt SHALL accept a relative file path as input from the user, resolved relative to the working directory where the Application was launched.
4. THE Path_Prompt SHALL pre-fill with the remote file's name as the default local filename.
5. WHEN the user confirms the path by pressing Enter, THE File_Copier SHALL resolve the path relative to the Application's launch working directory and transfer the remote file to that local path.
6. IF the specified local path already exists, THEN THE Path_Prompt SHALL display a confirmation asking whether to overwrite the existing file.
7. WHEN the file copy completes successfully, THE Application SHALL display a success message in the TUI status bar showing the resolved local path.
8. IF the local destination path's parent directory does not exist relative to the launch working directory, THEN THE File_Copier SHALL display an error message in the Path_Prompt without initiating the transfer.
9. IF the file transfer fails after starting, THEN THE File_Copier SHALL delete any partially written local file and display an error message in the TUI status bar.
10. WHEN the user presses Escape during the Path_Prompt, THE Application SHALL cancel the copy operation and return to the File_Browser.
11. WHILE a file transfer is in progress, THE Application SHALL display a transfer progress indicator in the TUI status bar showing bytes transferred.

### Requirement 6: TUI Layout and Status Feedback

**User Story:** As a user, I want a clear and informative terminal interface, so that I can understand the current state and available actions at all times.

#### Acceptance Criteria

1. THE Application SHALL render the TUI with three regions: a header bar showing the current absolute path and connection info (username@host:port), a main area showing the File_Browser listing, and a footer bar showing available key bindings.
2. WHEN a status message (error or success confirmation) is triggered, THE Application SHALL display it in the footer bar in place of the key bindings for a minimum of 3 seconds before restoring the key bindings display.
3. WHILE the Application is performing a remote operation (listing, downloading, copying), THE Application SHALL display a loading indicator in the footer bar.
4. THE Application SHALL use terminal alternate screen mode so that existing terminal content is preserved on exit.

### Requirement 7: Search Files

**User Story:** As a user, I want to search for files by name within the remote file system, so that I can quickly locate files without manually navigating through many directories.

#### Acceptance Criteria

1. WHEN the user presses Ctrl+F, THE Application SHALL display the Search_Prompt for a Local_Find operation limited to the currently displayed directory.
2. WHEN the user presses Ctrl+G, THE Application SHALL display the Search_Prompt for a Global_Find operation that recursively searches from the current directory downward.
3. WHEN the user confirms the search query by pressing Enter in the Search_Prompt, THE Application SHALL execute the corresponding search (Local_Find or Global_Find) and display matching Directory_Entry items in the Search_Results view.
4. THE Local_Find SHALL match file and directory names that contain the search query as a case-insensitive substring within the currently displayed directory only.
5. THE Global_Find SHALL match file and directory names that contain the search query as a case-insensitive substring, recursively traversing all subdirectories from the current directory downward.
6. THE Search_Results SHALL display each matching Directory_Entry with its relative path from the current directory, size, and type indicator.
7. WHEN the user presses Escape during an active Local_Find or Global_Find operation, THE Application SHALL abort the search immediately, discard partial results, and return to the File_Browser showing the previously displayed directory.
8. WHEN the user presses Escape while viewing Search_Results, THE Application SHALL close the Search_Results view and return to the File_Browser showing the previously displayed directory.
9. WHEN the user selects a file in the Search_Results and presses Enter, THE File_Browser SHALL navigate to the directory containing that file and place the selection cursor on the file.
10. WHEN the user selects a directory in the Search_Results and presses Enter, THE File_Browser SHALL navigate into that directory and display its contents.
11. WHILE a Global_Find operation is in progress, THE Application SHALL display a progress indicator showing the number of directories traversed so far, allowing the user to abort with Escape at any time.
12. IF no entries match the search query, THEN THE Search_Results SHALL display a no-results message indicating the query and search scope.
13. WHEN the user presses Escape while the Search_Prompt is displayed but before confirming, THE Application SHALL cancel the search and return to the File_Browser without executing a search.

### Requirement 8: Direct Path Navigation

**User Story:** As a user, I want to jump directly to a specific path on the remote server by typing it, so that I can navigate to known locations without stepping through each directory.

#### Acceptance Criteria

1. WHEN the user presses 'm', THE Application SHALL display a path input prompt (Navigate_Prompt) for the user to type a remote path.
2. THE Navigate_Prompt SHALL accept an absolute path on the remote server as input.
3. WHEN the user confirms the path by pressing Enter, THE File_Browser SHALL navigate to the specified path and display its contents.
4. IF the specified path is a directory, THEN THE File_Browser SHALL display the directory's contents with the cursor on the first entry.
5. IF the specified path is a file, THEN THE File_Browser SHALL navigate to the file's parent directory and place the cursor on that file.
6. IF the specified path does not exist or is not accessible, THEN THE Application SHALL display an error message in the TUI status bar and remain on the current directory.
7. WHEN the user presses Escape during the Navigate_Prompt, THE Application SHALL cancel the navigation and return to the File_Browser.
