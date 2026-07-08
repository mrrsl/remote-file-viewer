mod app;
mod config;
mod event;
mod operations;
mod ssh;
mod types;
mod ui;

use std::io::{self, stdout};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{App, AppMode, SearchType};
use config::AppConfig;
use event::{handle_key_event, Action};
use ssh::SshClient;
use types::{EntryType, StatusLevel, StatusMessage};

fn error_status(msg: String) -> StatusMessage {
    StatusMessage {
        text: msg,
        level: StatusLevel::Error,
        created_at: Instant::now(),
    }
}

fn success_status(msg: String) -> StatusMessage {
    StatusMessage {
        text: msg,
        level: StatusLevel::Success,
        created_at: Instant::now(),
    }
}

fn main() {
    // Parse command-line args: optional config path, default "./kafka-term-config"
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "./kafka-term-config".to_string());
    let config_path = PathBuf::from(&config_path);

    // Parse config
    let config = match AppConfig::from_file(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    // Establish SSH connection
    let ssh = match SshClient::connect(&config) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    // List home directory as initial content
    let home_path = PathBuf::from(format!("/home/{}", config.username));
    let (initial_path, initial_entries) = match operations::listing::list_directory(&ssh, &home_path, false) {
        Ok(entries) => (home_path, entries),
        Err(_) => {
            // Fall back to "/" if home directory fails
            let root = PathBuf::from("/");
            match operations::listing::list_directory(&ssh, &root, false) {
                Ok(entries) => (root, entries),
                Err(e) => {
                    eprintln!("Error: Failed to list initial directory: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };

    let mut app = App::new(initial_path, initial_entries);

    // Set up panic hook to restore terminal state
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));

    // Enter alternate screen and enable raw mode
    let mut stdout = stdout();
    if let Err(e) = enable_raw_mode() {
        eprintln!("Error: Failed to enable raw mode: {}", e);
        std::process::exit(1);
    }
    if let Err(e) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        eprintln!("Error: Failed to enter alternate screen: {}", e);
        std::process::exit(1);
    }

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = match Terminal::new(backend) {
        Ok(t) => t,
        Err(e) => {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            eprintln!("Error: Failed to create terminal: {}", e);
            std::process::exit(1);
        }
    };

    // Event loop
    let result = run_event_loop(&mut terminal, &mut app, &ssh, &config);

    // Cleanup: restore terminal
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);

    match result {
        Ok(()) => std::process::exit(0),
        Err(msg) => {
            eprintln!("Error: {}", msg);
            std::process::exit(1);
        }
    }
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    ssh: &SshClient,
    config: &AppConfig,
) -> Result<(), String> {
    loop {
        // Clear expired status messages
        app.clear_expired_status();

        // Render
        terminal
            .draw(|frame| ui::render(frame, app, &config.username, &config.ip_address))
            .map_err(|e| format!("Render error: {}", e))?;

        // Poll for events (with timeout for status message refresh)
        let poll_result = crossterm::event::poll(Duration::from_millis(250))
            .map_err(|e| format!("Event poll error: {}", e))?;

        if !poll_result {
            // Check connection on timeout
            if !ssh.is_connected() {
                return Err("SSH connection lost".to_string());
            }
            continue;
        }

        let evt = crossterm::event::read()
            .map_err(|e| format!("Event read error: {}", e))?;

        let Event::Key(key) = evt else {
            continue;
        };

        if key.kind != KeyEventKind::Press {
            continue;
        }

        let action = handle_key_event(key, &app.mode);

        match action {
            Action::Quit => return Ok(()),
            Action::MoveDown => {
                if matches!(app.mode, AppMode::SearchResults) {
                    app.move_search_cursor_down();
                } else {
                    app.move_cursor_down();
                }
            }
            Action::MoveUp => {
                if matches!(app.mode, AppMode::SearchResults) {
                    app.move_search_cursor_up();
                } else {
                    app.move_cursor_up();
                }
            }
            Action::Enter => {
                handle_enter(app, ssh);
            }
            Action::GoParent => {
                let parent = operations::listing::parent_path(&app.current_path);
                if parent != app.current_path {
                    match operations::listing::list_directory(ssh, &parent, app.show_hidden) {
                        Ok(entries) => {
                            app.current_path = parent;
                            app.entries = entries;
                            app.selected_index = 0;
                        }
                        Err(e) => app.set_status(error_status(e.to_string())),
                    }
                }
            }
            Action::ToggleHidden => {
                app.show_hidden = !app.show_hidden;
                match operations::listing::list_directory(ssh, &app.current_path, app.show_hidden) {
                    Ok(entries) => {
                        app.entries = entries;
                        app.selected_index = 0;
                    }
                    Err(e) => app.set_status(error_status(e.to_string())),
                }
            }
            Action::OpenFile => {
                if let Some(entry) = app.selected_entry() {
                    if entry.entry_type != EntryType::Directory {
                        let remote_path = entry.path.clone();
                        // Suspend TUI
                        let _ = disable_raw_mode();
                        let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
                        // View file
                        if let Err(msg) = operations::download::view_file(ssh, &remote_path) {
                            app.set_status(error_status(msg));
                        }
                        // Resume TUI
                        let _ = enable_raw_mode();
                        let _ = execute!(terminal.backend_mut(), EnterAlternateScreen);
                        let _ = terminal.clear();
                    }
                }
            }
            Action::CopyFile => {
                if let Some(entry) = app.selected_entry() {
                    if entry.entry_type != EntryType::Directory {
                        app.start_copy_prompt(entry.name.clone());
                    }
                }
            }
            Action::StartLocalFind => app.start_search(SearchType::Local),
            Action::StartGlobalFind => app.start_search(SearchType::Global),
            Action::StartNavigate => app.start_navigate(),
            Action::CharInput(c) => {
                match app.mode {
                    AppMode::SearchPrompt { .. } => app.append_search_char(c),
                    AppMode::PathPrompt => app.append_path_char(c),
                    AppMode::NavigatePrompt => app.append_navigate_char(c),
                    _ => {}
                }
            }
            Action::Backspace => {
                match app.mode {
                    AppMode::SearchPrompt { .. } => app.delete_search_char(),
                    AppMode::PathPrompt => app.delete_path_char(),
                    AppMode::NavigatePrompt => app.delete_navigate_char(),
                    _ => {}
                }
            }
            Action::ConfirmInput => {
                handle_confirm_input(app, ssh);
            }
            Action::CancelInput => {
                match app.mode {
                    AppMode::SearchPrompt { .. } | AppMode::SearchResults => app.cancel_search(),
                    AppMode::PathPrompt => app.cancel_path_prompt(),
                    AppMode::NavigatePrompt => app.cancel_navigate(),
                    _ => {}
                }
            }
            Action::AbortSearch => app.cancel_search(),
            Action::ConfirmOverwrite => {
                if let AppMode::OverwriteConfirm { ref path } = app.mode {
                    let local_path = path.clone();
                    let remote_path = app.selected_entry().map(|e| e.path.clone());
                    app.mode = AppMode::Browsing;
                    app.path_prompt_state = None;
                    if let Some(rp) = remote_path {
                        match operations::download::copy_remote_file(ssh, &rp, &local_path) {
                            Ok(bytes) => app.set_status(success_status(format!(
                                "Copied {} to {}",
                                types::format_size(bytes),
                                local_path.display()
                            ))),
                            Err(msg) => app.set_status(error_status(msg)),
                        }
                    }
                }
            }
            Action::DenyOverwrite => {
                app.mode = AppMode::Browsing;
                app.path_prompt_state = None;
            }
            Action::None => {}
        }

        // Check if SSH connection is still alive
        if !ssh.is_connected() {
            return Err("SSH connection lost".to_string());
        }
    }
}

fn handle_enter(app: &mut App, ssh: &SshClient) {
    match app.mode {
        AppMode::Browsing => {
            if let Some(entry) = app.selected_entry() {
                if entry.entry_type == EntryType::Directory {
                    let path = entry.path.clone();
                    match operations::listing::list_directory(ssh, &path, app.show_hidden) {
                        Ok(entries) => {
                            app.current_path = path;
                            app.entries = entries;
                            app.selected_index = 0;
                        }
                        Err(e) => app.set_status(error_status(e.to_string())),
                    }
                }
            }
        }
        AppMode::SearchResults => {
            if let Some(nav) = app.resolve_search_selection() {
                match operations::listing::list_directory(ssh, &nav.target_path, app.show_hidden) {
                    Ok(entries) => {
                        app.current_path = nav.target_path;
                        app.entries = entries;
                        app.selected_index = 0;
                        if let Some(filename) = nav.select_filename {
                            app.select_entry_by_name(&filename);
                        }
                        app.cancel_search();
                    }
                    Err(e) => app.set_status(error_status(e.to_string())),
                }
            }
        }
        _ => {}
    }
}

fn handle_confirm_input(app: &mut App, ssh: &SshClient) {
    match app.mode.clone() {
        AppMode::SearchPrompt { search_type } => {
            let query = app.search_query().unwrap_or("").to_string();
            if query.is_empty() {
                app.cancel_search();
            } else {
                match search_type {
                    SearchType::Local => {
                        match operations::search::local_find(
                            ssh,
                            &app.current_path,
                            &query,
                            app.show_hidden,
                        ) {
                            Ok(results) => app.set_search_results(results),
                            Err(e) => {
                                app.cancel_search();
                                app.set_status(error_status(e.to_string()));
                            }
                        }
                    }
                    SearchType::Global => {
                        match operations::search::global_find(
                            ssh,
                            &app.current_path,
                            &query,
                            app.show_hidden,
                            |_| true,
                        ) {
                            Ok(results) => app.set_search_results(results),
                            Err(e) => {
                                app.cancel_search();
                                app.set_status(error_status(e.to_string()));
                            }
                        }
                    }
                }
            }
        }
        AppMode::PathPrompt => {
            let input = app.get_path_input().unwrap_or("").to_string();
            if input.is_empty() {
                app.cancel_path_prompt();
            } else {
                let working_dir = std::env::current_dir().unwrap_or_default();
                let local_path = operations::listing::resolve_relative(&input, &working_dir);

                if let Err(msg) = operations::download::validate_copy_destination(&local_path) {
                    app.set_status(error_status(msg));
                    app.cancel_path_prompt();
                } else if local_path.exists() {
                    app.mode = AppMode::OverwriteConfirm { path: local_path };
                } else {
                    // Do the copy
                    let remote_path = app.selected_entry().map(|e| e.path.clone());
                    app.cancel_path_prompt();
                    if let Some(rp) = remote_path {
                        match operations::download::copy_remote_file(ssh, &rp, &local_path) {
                            Ok(bytes) => app.set_status(success_status(format!(
                                "Copied {} to {}",
                                types::format_size(bytes),
                                local_path.display()
                            ))),
                            Err(msg) => app.set_status(error_status(msg)),
                        }
                    }
                }
            }
        }
        AppMode::NavigatePrompt => {
            let input = app.get_navigate_input().unwrap_or("").to_string();
            if input.is_empty() {
                app.cancel_navigate();
            } else if !operations::navigate::validate_absolute_path(&input) {
                app.set_status(error_status(
                    "Path must be absolute (start with '/')".to_string(),
                ));
                app.cancel_navigate();
            } else {
                let path = PathBuf::from(&input);
                match operations::navigate::resolve_navigate_target(ssh, &path) {
                    Ok(operations::navigate::NavigateTarget::Directory(dir)) => {
                        match operations::listing::list_directory(ssh, &dir, app.show_hidden) {
                            Ok(entries) => {
                                app.current_path = dir;
                                app.entries = entries;
                                app.selected_index = 0;
                            }
                            Err(e) => app.set_status(error_status(e.to_string())),
                        }
                        app.cancel_navigate();
                    }
                    Ok(operations::navigate::NavigateTarget::File { parent, filename }) => {
                        match operations::listing::list_directory(ssh, &parent, app.show_hidden) {
                            Ok(entries) => {
                                app.current_path = parent;
                                app.entries = entries;
                                app.select_entry_by_name(&filename);
                            }
                            Err(e) => app.set_status(error_status(e.to_string())),
                        }
                        app.cancel_navigate();
                    }
                    Err(e) => {
                        app.set_status(error_status(e.to_string()));
                        app.cancel_navigate();
                    }
                }
            }
        }
        _ => {}
    }
}
