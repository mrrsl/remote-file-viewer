// Top-level UI render function

pub mod header;
pub mod browser;
pub mod footer;
pub mod search_prompt;
pub mod path_prompt;
pub mod navigate_prompt;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{App, AppMode};

use self::browser::render_browser;
use self::footer::render_footer;
use self::header::render_header;
use self::navigate_prompt::render_navigate_prompt;
use self::path_prompt::{render_copy_progress, render_overwrite_confirm, render_path_prompt};
use self::search_prompt::render_search_prompt;

/// Render the full application UI based on the current app state.
///
/// Composes the layout into three regions:
/// - Header (1 line): current path + connection info
/// - Main area (fill): file browser or search results
/// - Footer (1 line): key bindings or status messages
///
/// Dispatches to the appropriate widgets based on `app.mode`.
pub fn render(frame: &mut Frame, app: &App, username: &str, host: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Min(1),   // Main area
            Constraint::Length(1), // Footer
        ])
        .split(frame.area());

    let header_area = chunks[0];
    let main_area = chunks[1];
    let footer_area = chunks[2];

    // Header is always rendered
    render_header(frame, header_area, &app.current_path, username, host);

    match &app.mode {
        AppMode::Browsing => {
            render_browser(frame, main_area, &app.entries, app.selected_index);
            render_footer(
                frame,
                footer_area,
                app.status_message.as_ref(),
                app.loading,
            );
        }

        AppMode::SearchPrompt { search_type } => {
            // Split main area: browser on top, search prompt (3 lines) at bottom
            let prompt_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(3)])
                .split(main_area);

            render_browser(frame, prompt_chunks[0], &app.entries, app.selected_index);

            let query = app
                .search_state
                .as_ref()
                .map(|s| s.query.as_str())
                .unwrap_or("");
            render_search_prompt(frame, prompt_chunks[1], query, search_type, None);

            render_footer(
                frame,
                footer_area,
                app.status_message.as_ref(),
                app.loading,
            );
        }

        AppMode::Searching {
            search_type,
            progress,
        } => {
            // Split main area: browser on top, search prompt with progress at bottom
            let prompt_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(3)])
                .split(main_area);

            render_browser(frame, prompt_chunks[0], &app.entries, app.selected_index);

            let query = app
                .search_state
                .as_ref()
                .map(|s| s.query.as_str())
                .unwrap_or("");
            render_search_prompt(
                frame,
                prompt_chunks[1],
                query,
                search_type,
                Some(*progress),
            );

            render_footer(
                frame,
                footer_area,
                app.status_message.as_ref(),
                app.loading,
            );
        }

        AppMode::SearchResults => {
            // Display search results using the browser widget
            if let Some(ref search_state) = app.search_state {
                if search_state.results.is_empty() {
                    // Show no-results message
                    let msg = format!("No results found for '{}'", search_state.query);
                    let paragraph = Paragraph::new(Line::from(vec![Span::styled(
                        msg,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::ITALIC),
                    )]))
                    .alignment(Alignment::Center);
                    frame.render_widget(paragraph, main_area);
                } else {
                    render_browser(
                        frame,
                        main_area,
                        &search_state.results,
                        search_state.selected_index,
                    );
                }
            } else {
                // Fallback: no search state available
                let paragraph = Paragraph::new("No search results")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::DarkGray));
                frame.render_widget(paragraph, main_area);
            }

            render_footer(
                frame,
                footer_area,
                app.status_message.as_ref(),
                app.loading,
            );
        }

        AppMode::PathPrompt => {
            // Split main area: browser on top, path prompt (3 lines) at bottom
            let prompt_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(3)])
                .split(main_area);

            render_browser(frame, prompt_chunks[0], &app.entries, app.selected_index);

            let (input, default_name) = app
                .path_prompt_state
                .as_ref()
                .map(|s| (s.input.as_str(), s.default_name.as_str()))
                .unwrap_or(("", ""));
            render_path_prompt(frame, prompt_chunks[1], input, default_name);

            render_footer(
                frame,
                footer_area,
                app.status_message.as_ref(),
                app.loading,
            );
        }

        AppMode::OverwriteConfirm { path } => {
            // Split main area: browser on top, overwrite confirm (3 lines) at bottom
            let prompt_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(3)])
                .split(main_area);

            render_browser(frame, prompt_chunks[0], &app.entries, app.selected_index);
            render_overwrite_confirm(frame, prompt_chunks[1], path);

            render_footer(
                frame,
                footer_area,
                app.status_message.as_ref(),
                app.loading,
            );
        }

        AppMode::Copying { bytes_transferred } => {
            // Split main area: browser on top, copy progress (3 lines) at bottom
            let prompt_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(3)])
                .split(main_area);

            render_browser(frame, prompt_chunks[0], &app.entries, app.selected_index);
            render_copy_progress(frame, prompt_chunks[1], *bytes_transferred);

            render_footer(
                frame,
                footer_area,
                app.status_message.as_ref(),
                app.loading,
            );
        }

        AppMode::NavigatePrompt => {
            // Split main area: browser on top, navigate prompt (3 lines) at bottom
            let prompt_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(3)])
                .split(main_area);

            render_browser(frame, prompt_chunks[0], &app.entries, app.selected_index);

            let input = app
                .navigate_prompt_state
                .as_ref()
                .map(|s| s.input.as_str())
                .unwrap_or("");
            render_navigate_prompt(frame, prompt_chunks[1], input);

            render_footer(
                frame,
                footer_area,
                app.status_message.as_ref(),
                app.loading,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, AppMode, SearchState, SearchType, PathPromptState, NavigatePromptState};
    use crate::types::{DirectoryEntry, EntryType};
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
    use std::path::PathBuf;

    fn make_entry(name: &str, entry_type: EntryType, size: u64) -> DirectoryEntry {
        DirectoryEntry {
            name: name.to_string(),
            path: PathBuf::from(format!("/remote/{}", name)),
            entry_type,
            size,
        }
    }

    fn sample_entries() -> Vec<DirectoryEntry> {
        vec![
            make_entry("docs", EntryType::Directory, 0),
            make_entry("readme.md", EntryType::File, 1024),
            make_entry("link", EntryType::Symlink, 0),
        ]
    }

    fn buffer_to_string(buf: &Buffer) -> String {
        let area = buf.area();
        let mut result = String::new();
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                result.push_str(buf[(x, y)].symbol());
            }
        }
        result
    }

    #[test]
    fn test_render_browsing_mode() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let app = App::new(PathBuf::from("/home/user"), sample_entries());

        terminal
            .draw(|frame| {
                render(frame, &app, "deploy", "192.168.1.1");
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);

        // Header should show path and connection info
        assert!(content.contains("/home/user"));
        assert!(content.contains("deploy@192.168.1.1"));

        // Browser should show entries
        assert!(content.contains("docs"));
        assert!(content.contains("readme.md"));

        // Footer should show key bindings
        assert!(content.contains("quit"));
    }

    #[test]
    fn test_render_search_results_empty() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new(PathBuf::from("/home/user"), sample_entries());
        app.mode = AppMode::SearchResults;
        app.search_state = Some(SearchState {
            search_type: SearchType::Local,
            query: "nonexistent".to_string(),
            results: vec![],
            selected_index: 0,
        });

        terminal
            .draw(|frame| {
                render(frame, &app, "deploy", "192.168.1.1");
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);

        // Should show no-results message
        assert!(
            content.contains("No results found for 'nonexistent'"),
            "Expected no-results message, got: {}",
            content
        );
    }

    #[test]
    fn test_render_search_results_with_entries() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let results = vec![
            make_entry("found.log", EntryType::File, 2048),
            make_entry("found_dir", EntryType::Directory, 0),
        ];

        let mut app = App::new(PathBuf::from("/home/user"), sample_entries());
        app.mode = AppMode::SearchResults;
        app.search_state = Some(SearchState {
            search_type: SearchType::Global,
            query: "found".to_string(),
            results,
            selected_index: 0,
        });

        terminal
            .draw(|frame| {
                render(frame, &app, "deploy", "192.168.1.1");
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);

        // Should show the search results
        assert!(content.contains("found.log"));
        assert!(content.contains("found_dir"));
    }

    #[test]
    fn test_render_navigate_prompt_mode() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new(PathBuf::from("/home/user"), sample_entries());
        app.mode = AppMode::NavigatePrompt;
        app.navigate_prompt_state = Some(NavigatePromptState {
            input: "/var/log".to_string(),
        });

        terminal
            .draw(|frame| {
                render(frame, &app, "deploy", "192.168.1.1");
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);

        // Should show the navigate prompt with input
        assert!(content.contains("Navigate to:"));
        assert!(content.contains("/var/log"));
    }

    #[test]
    fn test_render_path_prompt_mode() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut app = App::new(PathBuf::from("/home/user"), sample_entries());
        app.mode = AppMode::PathPrompt;
        app.path_prompt_state = Some(PathPromptState {
            input: "output.log".to_string(),
            default_name: "output.log".to_string(),
        });

        terminal
            .draw(|frame| {
                render(frame, &app, "deploy", "192.168.1.1");
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);

        // Should show the copy prompt
        assert!(content.contains("Copy to:"));
        assert!(content.contains("output.log"));
    }
}
