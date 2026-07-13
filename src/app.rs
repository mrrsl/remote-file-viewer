// Application state machine

use std::path::PathBuf;
use std::time::Duration;

use crate::types::{DirectoryEntry, EntryType, StatusMessage};

/// The type of search being performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchType {
    Local,
    Global,
}

/// Application mode representing the current state of the TUI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    Browsing,
    SearchPrompt { search_type: SearchType },
    Searching { search_type: SearchType, progress: usize },
    SearchResults,
    PathPrompt,
    OverwriteConfirm { path: PathBuf },
    Copying { bytes_transferred: u64 },
    NavigatePrompt,
    DirectoryCopyConfirm { path: PathBuf, size: u64 },
}

/// State for an active or completed search operation.
#[derive(Debug, Clone)]
pub struct SearchState {
    pub search_type: SearchType,
    pub query: String,
    pub results: Vec<DirectoryEntry>,
    pub selected_index: usize,
}

/// State for the file copy path prompt.
#[derive(Debug, Clone)]
pub struct PathPromptState {
    pub input: String,
    pub default_name: String,
}

/// State for the direct path navigation prompt.
#[derive(Debug, Clone)]
pub struct NavigatePromptState {
    pub input: String,
}

/// Describes where to navigate after selecting a search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchNavigation {
    /// The directory path to navigate to.
    pub target_path: PathBuf,
    /// If Some, place cursor on this filename after listing the directory.
    pub select_filename: Option<String>,
}

/// Main application state.
pub struct App {
    pub mode: AppMode,
    pub current_path: PathBuf,
    pub entries: Vec<DirectoryEntry>,
    pub selected_index: usize,
    pub show_hidden: bool,
    pub status_message: Option<StatusMessage>,
    pub search_state: Option<SearchState>,
    pub path_prompt_state: Option<PathPromptState>,
    pub navigate_prompt_state: Option<NavigatePromptState>,
    pub loading: bool,
}

impl App {
    /// Create a new App with the given initial path and directory entries.
    pub fn new(initial_path: PathBuf, entries: Vec<DirectoryEntry>) -> Self {
        Self {
            mode: AppMode::Browsing,
            current_path: initial_path,
            entries,
            selected_index: 0,
            show_hidden: false,
            status_message: None,
            search_state: None,
            path_prompt_state: None,
            navigate_prompt_state: None,
            loading: false,
        }
    }

    /// Move the selection cursor down by one entry. Clamps at the last entry.
    pub fn move_cursor_down(&mut self) {
        if !self.entries.is_empty() && self.selected_index < self.entries.len() - 1 {
            self.selected_index += 1;
        }
    }

    /// Move the selection cursor up by one entry. Clamps at the first entry.
    pub fn move_cursor_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Returns a reference to the currently selected directory entry, if any.
    pub fn selected_entry(&self) -> Option<&DirectoryEntry> {
        self.entries.get(self.selected_index)
    }

    /// Set the current status message.
    pub fn set_status(&mut self, message: StatusMessage) {
        self.status_message = Some(message);
    }

    /// Clear the status message if it has been displayed for at least 3 seconds.
    pub fn clear_expired_status(&mut self) {
        if let Some(ref msg) = self.status_message {
            if msg.created_at.elapsed() >= Duration::from_secs(3) {
                self.status_message = None;
            }
        }
    }

    /// Set the cursor to the entry matching the given filename, or 0 if not found.
    pub fn select_entry_by_name(&mut self, name: &str) {
        self.selected_index = self
            .entries
            .iter()
            .position(|e| e.name == name)
            .unwrap_or(0);
    }

    /// Start a search operation: set mode to SearchPrompt and initialize search_state.
    pub fn start_search(&mut self, search_type: SearchType) {
        self.search_state = Some(SearchState {
            search_type: search_type.clone(),
            query: String::new(),
            results: Vec::new(),
            selected_index: 0,
        });
        self.mode = AppMode::SearchPrompt { search_type };
    }

    /// Append a character to the current search query.
    /// Does nothing if no search_state is active.
    pub fn append_search_char(&mut self, c: char) {
        if let Some(ref mut state) = self.search_state {
            state.query.push(c);
        }
    }

    /// Delete the last character from the current search query.
    /// Does nothing if no search_state is active or query is empty.
    pub fn delete_search_char(&mut self) {
        if let Some(ref mut state) = self.search_state {
            state.query.pop();
        }
    }

    /// Store search results and switch to SearchResults mode.
    pub fn set_search_results(&mut self, results: Vec<DirectoryEntry>) {
        if let Some(ref mut state) = self.search_state {
            state.results = results;
            state.selected_index = 0;
        }
        self.mode = AppMode::SearchResults;
    }

    /// Cancel the current search and return to Browsing mode.
    /// Clears search_state entirely.
    pub fn cancel_search(&mut self) {
        self.search_state = None;
        self.mode = AppMode::Browsing;
    }

    /// Move the search results cursor up by one entry. Clamps at index 0.
    pub fn move_search_cursor_up(&mut self) {
        if let Some(ref mut state) = self.search_state {
            if state.selected_index > 0 {
                state.selected_index -= 1;
            }
        }
    }

    /// Move the search results cursor down by one entry. Clamps at the last result.
    pub fn move_search_cursor_down(&mut self) {
        if let Some(ref mut state) = self.search_state {
            if !state.results.is_empty() && state.selected_index < state.results.len() - 1 {
                state.selected_index += 1;
            }
        }
    }

    /// Returns a reference to the currently selected search result entry, if any.
    pub fn selected_search_entry(&self) -> Option<&DirectoryEntry> {
        self.search_state
            .as_ref()
            .and_then(|state| state.results.get(state.selected_index))
    }

    /// Get the current search query, if a search is active.
    pub fn search_query(&self) -> Option<&str> {
        self.search_state.as_ref().map(|s| s.query.as_str())
    }

    /// Get the current search type, if a search is active.
    pub fn search_type(&self) -> Option<&SearchType> {
        self.search_state.as_ref().map(|s| &s.search_type)
    }

    /// Set mode to Searching with progress indicator.
    pub fn set_searching(&mut self, search_type: SearchType) {
        self.mode = AppMode::Searching { search_type, progress: 0 };
    }

    /// Update the search progress counter (directories traversed).
    pub fn update_search_progress(&mut self, progress: usize) {
        if let AppMode::Searching { progress: ref mut p, .. } = self.mode {
            *p = progress;
        }
    }

    /// Navigate to a search result. If the result is a directory, sets up navigation
    /// into it. If a file, navigates to parent directory with cursor on the file.
    /// Returns the target path to navigate to and optionally a filename to select.
    pub fn resolve_search_selection(&self) -> Option<SearchNavigation> {
        let entry = self.selected_search_entry()?;
        match entry.entry_type {
            EntryType::Directory => Some(SearchNavigation {
                target_path: entry.path.clone(),
                select_filename: None,
            }),
            EntryType::File | EntryType::Symlink => {
                let parent = entry.path.parent()?.to_path_buf();
                Some(SearchNavigation {
                    target_path: parent,
                    select_filename: Some(entry.name.clone()),
                })
            }
        }
    }

    // --- Navigate prompt state management ---

    /// Enter NavigatePrompt mode with an empty input buffer.
    pub fn start_navigate(&mut self) {
        self.mode = AppMode::NavigatePrompt;
        self.navigate_prompt_state = Some(NavigatePromptState {
            input: String::new(),
        });
    }

    /// Append a character to the navigate prompt input.
    pub fn append_navigate_char(&mut self, c: char) {
        if let Some(ref mut state) = self.navigate_prompt_state {
            state.input.push(c);
        }
    }

    /// Delete the last character from the navigate prompt input.
    pub fn delete_navigate_char(&mut self) {
        if let Some(ref mut state) = self.navigate_prompt_state {
            state.input.pop();
        }
    }

    /// Cancel the navigate prompt and return to Browsing mode.
    pub fn cancel_navigate(&mut self) {
        self.mode = AppMode::Browsing;
        self.navigate_prompt_state = None;
    }

    /// Get the current navigate prompt input, if active.
    pub fn get_navigate_input(&self) -> Option<&str> {
        self.navigate_prompt_state.as_ref().map(|s| s.input.as_str())
    }

    // --- Path prompt (copy) state management ---

    /// Enter PathPrompt mode with the default filename pre-filled.
    pub fn start_copy_prompt(&mut self, default_name: String) {
        self.mode = AppMode::PathPrompt;
        self.path_prompt_state = Some(PathPromptState {
            input: default_name.clone(),
            default_name,
        });
    }

    /// Append a character to the path prompt input.
    pub fn append_path_char(&mut self, c: char) {
        if let Some(ref mut state) = self.path_prompt_state {
            state.input.push(c);
        }
    }

    /// Delete the last character from the path prompt input.
    pub fn delete_path_char(&mut self) {
        if let Some(ref mut state) = self.path_prompt_state {
            state.input.pop();
        }
    }

    /// Cancel the path prompt and return to Browsing mode.
    pub fn cancel_path_prompt(&mut self) {
        self.mode = AppMode::Browsing;
        self.path_prompt_state = None;
    }

    /// Get the current path prompt input, if active.
    pub fn get_path_input(&self) -> Option<&str> {
        self.path_prompt_state.as_ref().map(|s| s.input.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EntryType, StatusLevel};
    use std::time::Instant;

    fn make_entry(name: &str, entry_type: EntryType) -> DirectoryEntry {
        DirectoryEntry {
            name: name.to_string(),
            path: PathBuf::from(format!("/test/{}", name)),
            entry_type,
            size: 0,
        }
    }

    fn sample_entries() -> Vec<DirectoryEntry> {
        vec![
            make_entry("alpha", EntryType::Directory),
            make_entry("beta.txt", EntryType::File),
            make_entry("gamma", EntryType::Directory),
        ]
    }

    #[test]
    fn test_new_app_defaults() {
        let entries = sample_entries();
        let app = App::new(PathBuf::from("/home/user"), entries.clone());

        assert_eq!(app.mode, AppMode::Browsing);
        assert_eq!(app.current_path, PathBuf::from("/home/user"));
        assert_eq!(app.selected_index, 0);
        assert!(!app.show_hidden);
        assert!(app.status_message.is_none());
        assert!(app.search_state.is_none());
        assert!(app.path_prompt_state.is_none());
        assert!(app.navigate_prompt_state.is_none());
        assert!(!app.loading);
        assert_eq!(app.entries.len(), 3);
    }

    #[test]
    fn test_move_cursor_down() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        assert_eq!(app.selected_index, 0);

        app.move_cursor_down();
        assert_eq!(app.selected_index, 1);

        app.move_cursor_down();
        assert_eq!(app.selected_index, 2);

        // At last entry, should clamp
        app.move_cursor_down();
        assert_eq!(app.selected_index, 2);
    }

    #[test]
    fn test_move_cursor_up() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.selected_index = 2;

        app.move_cursor_up();
        assert_eq!(app.selected_index, 1);

        app.move_cursor_up();
        assert_eq!(app.selected_index, 0);

        // At first entry, should clamp
        app.move_cursor_up();
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_move_cursor_empty_entries() {
        let mut app = App::new(PathBuf::from("/"), vec![]);
        app.move_cursor_down();
        assert_eq!(app.selected_index, 0);

        app.move_cursor_up();
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_selected_entry() {
        let app = App::new(PathBuf::from("/"), sample_entries());
        let entry = app.selected_entry().unwrap();
        assert_eq!(entry.name, "alpha");
    }

    #[test]
    fn test_selected_entry_empty() {
        let app = App::new(PathBuf::from("/"), vec![]);
        assert!(app.selected_entry().is_none());
    }

    #[test]
    fn test_set_status() {
        let mut app = App::new(PathBuf::from("/"), vec![]);
        let msg = StatusMessage {
            text: "File copied".to_string(),
            level: StatusLevel::Success,
            created_at: Instant::now(),
        };
        app.set_status(msg);
        assert!(app.status_message.is_some());
        assert_eq!(app.status_message.as_ref().unwrap().text, "File copied");
    }

    #[test]
    fn test_clear_expired_status_recent() {
        let mut app = App::new(PathBuf::from("/"), vec![]);
        let msg = StatusMessage {
            text: "test".to_string(),
            level: StatusLevel::Info,
            created_at: Instant::now(),
        };
        app.set_status(msg);
        app.clear_expired_status();
        // Should still be present since it was just created
        assert!(app.status_message.is_some());
    }

    #[test]
    fn test_clear_expired_status_old() {
        let mut app = App::new(PathBuf::from("/"), vec![]);
        let msg = StatusMessage {
            text: "test".to_string(),
            level: StatusLevel::Info,
            created_at: Instant::now() - Duration::from_secs(5),
        };
        app.set_status(msg);
        app.clear_expired_status();
        // Should be cleared since it's older than 3 seconds
        assert!(app.status_message.is_none());
    }

    #[test]
    fn test_select_entry_by_name_found() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.select_entry_by_name("gamma");
        assert_eq!(app.selected_index, 2);
    }

    #[test]
    fn test_select_entry_by_name_not_found() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.selected_index = 2;
        app.select_entry_by_name("nonexistent");
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_select_entry_by_name_first() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.selected_index = 2;
        app.select_entry_by_name("alpha");
        assert_eq!(app.selected_index, 0);
    }

    // --- Search operation tests ---

    #[test]
    fn test_start_search_local() {
        let mut app = App::new(PathBuf::from("/home"), sample_entries());
        app.start_search(SearchType::Local);

        assert_eq!(app.mode, AppMode::SearchPrompt { search_type: SearchType::Local });
        assert!(app.search_state.is_some());
        let state = app.search_state.as_ref().unwrap();
        assert_eq!(state.search_type, SearchType::Local);
        assert_eq!(state.query, "");
        assert!(state.results.is_empty());
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_start_search_global() {
        let mut app = App::new(PathBuf::from("/home"), sample_entries());
        app.start_search(SearchType::Global);

        assert_eq!(app.mode, AppMode::SearchPrompt { search_type: SearchType::Global });
        assert!(app.search_state.is_some());
        let state = app.search_state.as_ref().unwrap();
        assert_eq!(state.search_type, SearchType::Global);
    }

    #[test]
    fn test_append_search_char() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_search(SearchType::Local);

        app.append_search_char('h');
        app.append_search_char('e');
        app.append_search_char('l');

        assert_eq!(app.search_query(), Some("hel"));
    }

    #[test]
    fn test_append_search_char_no_state() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        // No search state active — should not panic
        app.append_search_char('x');
        assert!(app.search_state.is_none());
    }

    #[test]
    fn test_delete_search_char() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_search(SearchType::Local);

        app.append_search_char('a');
        app.append_search_char('b');
        app.delete_search_char();

        assert_eq!(app.search_query(), Some("a"));
    }

    #[test]
    fn test_delete_search_char_empty() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_search(SearchType::Local);

        // Delete on empty query should not panic
        app.delete_search_char();
        assert_eq!(app.search_query(), Some(""));
    }

    #[test]
    fn test_set_search_results() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_search(SearchType::Local);

        let results = vec![
            make_entry("found1.txt", EntryType::File),
            make_entry("found2", EntryType::Directory),
        ];
        app.set_search_results(results);

        assert_eq!(app.mode, AppMode::SearchResults);
        let state = app.search_state.as_ref().unwrap();
        assert_eq!(state.results.len(), 2);
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_cancel_search() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_search(SearchType::Global);
        app.append_search_char('x');

        app.cancel_search();

        assert_eq!(app.mode, AppMode::Browsing);
        assert!(app.search_state.is_none());
    }

    #[test]
    fn test_move_search_cursor_down() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_search(SearchType::Local);
        let results = vec![
            make_entry("a.txt", EntryType::File),
            make_entry("b.txt", EntryType::File),
            make_entry("c.txt", EntryType::File),
        ];
        app.set_search_results(results);

        app.move_search_cursor_down();
        assert_eq!(app.search_state.as_ref().unwrap().selected_index, 1);

        app.move_search_cursor_down();
        assert_eq!(app.search_state.as_ref().unwrap().selected_index, 2);

        // Clamps at last
        app.move_search_cursor_down();
        assert_eq!(app.search_state.as_ref().unwrap().selected_index, 2);
    }

    #[test]
    fn test_move_search_cursor_up() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_search(SearchType::Local);
        let results = vec![
            make_entry("a.txt", EntryType::File),
            make_entry("b.txt", EntryType::File),
        ];
        app.set_search_results(results);

        // Move to second item first
        app.move_search_cursor_down();
        assert_eq!(app.search_state.as_ref().unwrap().selected_index, 1);

        app.move_search_cursor_up();
        assert_eq!(app.search_state.as_ref().unwrap().selected_index, 0);

        // Clamps at first
        app.move_search_cursor_up();
        assert_eq!(app.search_state.as_ref().unwrap().selected_index, 0);
    }

    #[test]
    fn test_selected_search_entry() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_search(SearchType::Local);
        let results = vec![
            make_entry("first.txt", EntryType::File),
            make_entry("second", EntryType::Directory),
        ];
        app.set_search_results(results);

        let entry = app.selected_search_entry().unwrap();
        assert_eq!(entry.name, "first.txt");

        app.move_search_cursor_down();
        let entry = app.selected_search_entry().unwrap();
        assert_eq!(entry.name, "second");
    }

    #[test]
    fn test_selected_search_entry_no_state() {
        let app = App::new(PathBuf::from("/"), sample_entries());
        assert!(app.selected_search_entry().is_none());
    }

    #[test]
    fn test_selected_search_entry_empty_results() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_search(SearchType::Local);
        app.set_search_results(vec![]);
        assert!(app.selected_search_entry().is_none());
    }

    #[test]
    fn test_resolve_search_selection_directory() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_search(SearchType::Local);
        let results = vec![DirectoryEntry {
            name: "logs".to_string(),
            path: PathBuf::from("/home/user/logs"),
            entry_type: EntryType::Directory,
            size: 0,
        }];
        app.set_search_results(results);

        let nav = app.resolve_search_selection().unwrap();
        assert_eq!(nav.target_path, PathBuf::from("/home/user/logs"));
        assert_eq!(nav.select_filename, None);
    }

    #[test]
    fn test_resolve_search_selection_file() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_search(SearchType::Global);
        let results = vec![DirectoryEntry {
            name: "app.log".to_string(),
            path: PathBuf::from("/home/user/logs/app.log"),
            entry_type: EntryType::File,
            size: 4096,
        }];
        app.set_search_results(results);

        let nav = app.resolve_search_selection().unwrap();
        assert_eq!(nav.target_path, PathBuf::from("/home/user/logs"));
        assert_eq!(nav.select_filename, Some("app.log".to_string()));
    }

    #[test]
    fn test_resolve_search_selection_no_state() {
        let app = App::new(PathBuf::from("/"), sample_entries());
        assert!(app.resolve_search_selection().is_none());
    }

    #[test]
    fn test_set_searching() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.set_searching(SearchType::Global);
        assert_eq!(app.mode, AppMode::Searching { search_type: SearchType::Global, progress: 0 });
    }

    #[test]
    fn test_update_search_progress() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.set_searching(SearchType::Global);
        app.update_search_progress(42);
        assert_eq!(app.mode, AppMode::Searching { search_type: SearchType::Global, progress: 42 });
    }

    #[test]
    fn test_update_search_progress_wrong_mode() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        // Not in Searching mode — should not panic
        app.update_search_progress(10);
        assert_eq!(app.mode, AppMode::Browsing);
    }

    #[test]
    fn test_search_type_accessor() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        assert!(app.search_type().is_none());

        app.start_search(SearchType::Global);
        assert_eq!(app.search_type(), Some(&SearchType::Global));
    }

    // --- Navigate prompt tests ---

    #[test]
    fn test_start_navigate() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_navigate();
        assert_eq!(app.mode, AppMode::NavigatePrompt);
        assert!(app.navigate_prompt_state.is_some());
        assert_eq!(app.navigate_prompt_state.as_ref().unwrap().input, "");
    }

    #[test]
    fn test_append_navigate_char() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_navigate();
        app.append_navigate_char('/');
        app.append_navigate_char('h');
        app.append_navigate_char('o');
        app.append_navigate_char('m');
        app.append_navigate_char('e');
        assert_eq!(app.get_navigate_input(), Some("/home"));
    }

    #[test]
    fn test_delete_navigate_char() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_navigate();
        app.append_navigate_char('/');
        app.append_navigate_char('a');
        app.delete_navigate_char();
        assert_eq!(app.get_navigate_input(), Some("/"));
    }

    #[test]
    fn test_delete_navigate_char_empty() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_navigate();
        app.delete_navigate_char(); // Should not panic on empty
        assert_eq!(app.get_navigate_input(), Some(""));
    }

    #[test]
    fn test_cancel_navigate() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_navigate();
        app.append_navigate_char('/');
        app.cancel_navigate();
        assert_eq!(app.mode, AppMode::Browsing);
        assert!(app.navigate_prompt_state.is_none());
        assert_eq!(app.get_navigate_input(), None);
    }

    #[test]
    fn test_get_navigate_input_not_active() {
        let app = App::new(PathBuf::from("/"), sample_entries());
        assert_eq!(app.get_navigate_input(), None);
    }

    // --- Path prompt (copy) tests ---

    #[test]
    fn test_start_copy_prompt() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_copy_prompt("file.log".to_string());
        assert_eq!(app.mode, AppMode::PathPrompt);
        assert!(app.path_prompt_state.is_some());
        let state = app.path_prompt_state.as_ref().unwrap();
        assert_eq!(state.input, "file.log");
        assert_eq!(state.default_name, "file.log");
    }

    #[test]
    fn test_append_path_char() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_copy_prompt("file.log".to_string());
        app.append_path_char('.');
        app.append_path_char('b');
        app.append_path_char('k');
        assert_eq!(app.get_path_input(), Some("file.log.bk"));
    }

    #[test]
    fn test_delete_path_char() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_copy_prompt("file.log".to_string());
        app.delete_path_char();
        assert_eq!(app.get_path_input(), Some("file.lo"));
    }

    #[test]
    fn test_delete_path_char_empty() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_copy_prompt(String::new());
        app.delete_path_char(); // Should not panic on empty
        assert_eq!(app.get_path_input(), Some(""));
    }

    #[test]
    fn test_cancel_path_prompt() {
        let mut app = App::new(PathBuf::from("/"), sample_entries());
        app.start_copy_prompt("file.log".to_string());
        app.cancel_path_prompt();
        assert_eq!(app.mode, AppMode::Browsing);
        assert!(app.path_prompt_state.is_none());
        assert_eq!(app.get_path_input(), None);
    }

    #[test]
    fn test_get_path_input_not_active() {
        let app = App::new(PathBuf::from("/"), sample_entries());
        assert_eq!(app.get_path_input(), None);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::types::{DirectoryEntry, EntryType, StatusLevel, StatusMessage};
    use proptest::prelude::*;
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    // Feature: ssh-remote-file-browser, Property 12: Navigate cursor placement on file target
    // Validates: Requirements 8.5
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn select_entry_by_name_sets_correct_index(
            entry_count in 1usize..=100,
            seed in proptest::collection::vec("[a-zA-Z0-9]{1,20}", 1..=100),
            use_present_name in proptest::bool::ANY,
            absent_suffix in "[a-zA-Z0-9]{1,10}",
        ) {
            // Generate unique names from the seed
            let mut seen = HashSet::new();
            let unique_names: Vec<String> = seed.into_iter()
                .filter(|name| seen.insert(name.clone()))
                .take(entry_count)
                .collect();

            // Need at least 1 entry
            prop_assume!(!unique_names.is_empty());

            let entries: Vec<DirectoryEntry> = unique_names.iter().map(|name| {
                DirectoryEntry {
                    name: name.clone(),
                    path: PathBuf::from(format!("/test/{}", name)),
                    entry_type: EntryType::File,
                    size: 0,
                }
            }).collect();

            let mut app = App::new(PathBuf::from("/test"), entries.clone());

            if use_present_name {
                // Pick a name that IS in the list (use middle entry to avoid trivial index 0)
                let target_idx = unique_names.len() / 2;
                let target_name = &unique_names[target_idx];

                app.select_entry_by_name(target_name);

                prop_assert_eq!(
                    app.selected_index, target_idx,
                    "Expected cursor at index {} for name {:?}, got {}",
                    target_idx, target_name, app.selected_index
                );
            } else {
                // Generate a name that is NOT in the list
                let absent_name = format!("__absent__{}", absent_suffix);
                // Ensure it's truly absent
                prop_assume!(!unique_names.contains(&absent_name));

                app.selected_index = unique_names.len() / 2; // Start at non-zero position
                app.select_entry_by_name(&absent_name);

                prop_assert_eq!(
                    app.selected_index, 0,
                    "Expected cursor at 0 for absent name {:?}, got {}",
                    absent_name, app.selected_index
                );
            }
        }
    }

    // Feature: ssh-remote-file-browser, Property 5: Cursor movement bounds
    // Validates: Requirements 3.1, 3.2, 3.3, 3.4

    fn make_dummy_entry(index: usize) -> DirectoryEntry {
        DirectoryEntry {
            name: format!("entry_{}", index),
            path: PathBuf::from(format!("/dummy/entry_{}", index)),
            entry_type: EntryType::File,
            size: 0,
        }
    }

    fn make_dummy_entries(n: usize) -> Vec<DirectoryEntry> {
        (0..n).map(make_dummy_entry).collect()
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn cursor_move_down_bounds(
            n in 1usize..=1000,
            i_frac in 0.0f64..1.0,
        ) {
            let i = ((i_frac * n as f64) as usize).min(n - 1);
            let entries = make_dummy_entries(n);
            let mut app = App::new(PathBuf::from("/"), entries);
            app.selected_index = i;

            app.move_cursor_down();

            if i < n - 1 {
                prop_assert_eq!(app.selected_index, i + 1,
                    "move_down from {} with N={} should yield {}, got {}",
                    i, n, i + 1, app.selected_index);
            } else {
                prop_assert_eq!(app.selected_index, n - 1,
                    "move_down from last position {} with N={} should clamp at {}, got {}",
                    i, n, n - 1, app.selected_index);
            }

            // Invariant: selected_index is always in [0, N-1]
            prop_assert!(app.selected_index < n,
                "selected_index {} should be < N={}", app.selected_index, n);
        }

        #[test]
        fn cursor_move_up_bounds(
            n in 1usize..=1000,
            i_frac in 0.0f64..1.0,
        ) {
            let i = ((i_frac * n as f64) as usize).min(n - 1);
            let entries = make_dummy_entries(n);
            let mut app = App::new(PathBuf::from("/"), entries);
            app.selected_index = i;

            app.move_cursor_up();

            if i > 0 {
                prop_assert_eq!(app.selected_index, i - 1,
                    "move_up from {} with N={} should yield {}, got {}",
                    i, n, i - 1, app.selected_index);
            } else {
                prop_assert_eq!(app.selected_index, 0,
                    "move_up from first position {} with N={} should clamp at 0, got {}",
                    i, n, app.selected_index);
            }

            // Invariant: selected_index is always in [0, N-1]
            prop_assert!(app.selected_index < n,
                "selected_index {} should be < N={}", app.selected_index, n);
        }

        #[test]
        fn cursor_always_in_bounds_after_movement(
            n in 1usize..=1000,
            i_frac in 0.0f64..1.0,
        ) {
            let i = ((i_frac * n as f64) as usize).min(n - 1);
            let entries = make_dummy_entries(n);

            // Test move_down
            let mut app_down = App::new(PathBuf::from("/"), entries.clone());
            app_down.selected_index = i;
            app_down.move_cursor_down();
            prop_assert!(app_down.selected_index < n,
                "After move_down: selected_index {} should be in [0, {})", app_down.selected_index, n);

            // Test move_up
            let mut app_up = App::new(PathBuf::from("/"), entries);
            app_up.selected_index = i;
            app_up.move_cursor_up();
            prop_assert!(app_up.selected_index < n,
                "After move_up: selected_index {} should be in [0, {})", app_up.selected_index, n);
        }
    }

    // Feature: ssh-remote-file-browser, Property 8: Status message visibility
    // Validates: Requirements 6.2
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn status_message_visibility(ms in 0u64..=10000) {
            let created_at = Instant::now() - Duration::from_millis(ms);
            let msg = StatusMessage {
                text: "test message".to_string(),
                level: StatusLevel::Info,
                created_at,
            };

            let mut app = App::new(PathBuf::from("/"), vec![]);
            app.set_status(msg);

            app.clear_expired_status();

            if ms < 3000 {
                // Message should still be visible (not cleared)
                prop_assert!(
                    app.status_message.is_some(),
                    "Status message should be visible when elapsed {}ms < 3000ms",
                    ms
                );
            } else {
                // Message should be cleared
                prop_assert!(
                    app.status_message.is_none(),
                    "Status message should be cleared when elapsed {}ms >= 3000ms",
                    ms
                );
            }
        }
    }
}
