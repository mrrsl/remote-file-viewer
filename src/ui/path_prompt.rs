// Copy path input widget

use std::path::Path;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::types::format_size;

/// Render the path prompt widget for file copy destination input.
///
/// Displays a bordered input area titled " Copy File " with:
/// - A "Copy to: " label in bold
/// - The current input text (pre-filled with the remote filename as default)
/// - A block cursor indicator (█) at the end of the input
pub fn render_path_prompt(frame: &mut Frame, area: Rect, input: &str, _default_name: &str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Copy File ");

    let label = Span::styled(
        "Copy to: ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let input_text = Span::styled(input, Style::default().fg(Color::White));

    let cursor = Span::styled(
        "█",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::SLOW_BLINK),
    );

    let line = Line::from(vec![label, input_text, cursor]);

    let paragraph = Paragraph::new(line).block(block);
    frame.render_widget(paragraph, area);
}

/// Render the overwrite confirmation prompt.
///
/// Displays a bordered area titled " Copy File " with:
/// - "File exists. Overwrite? (y/n): " message with the path displayed
pub fn render_overwrite_confirm(frame: &mut Frame, area: Rect, path: &Path) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Copy File ");

    let message = format!("File exists. Overwrite? (y/n): {}", path.display());

    let line = Line::from(vec![Span::styled(
        message,
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]);

    let paragraph = Paragraph::new(line).block(block);
    frame.render_widget(paragraph, area);
}

/// Render the copy progress indicator.
///
/// Displays a bordered area titled " Copy File " with:
/// - "Copying... X transferred" where X is formatted using format_size
pub fn render_copy_progress(frame: &mut Frame, area: Rect, bytes_transferred: u64) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .title(" Copy File ");

    let progress_text = format!("Copying... {} transferred", format_size(bytes_transferred));

    let line = Line::from(vec![Span::styled(
        progress_text,
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::ITALIC),
    )]);

    let paragraph = Paragraph::new(line).block(block);
    frame.render_widget(paragraph, area);
}

/// Render the directory copy confirmation prompt.
///
/// Displays a bordered area titled " Download Directory " with:
/// - The remote directory path
/// - The total size formatted using format_size
/// - Key hints: 'y' to confirm, 'n' or Escape to cancel
pub fn render_dir_copy_confirm(frame: &mut Frame, area: Rect, path: &Path, size: u64) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Download Directory ");

    let path_span = Span::styled(
        format!("Path: {}", path.display()),
        Style::default().fg(Color::White),
    );

    let size_span = Span::styled(
        format!("  Size: {}", format_size(size)),
        Style::default().fg(Color::White),
    );

    let hint_span = Span::styled(
        "  [y] confirm  [n/Esc] cancel",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let line = Line::from(vec![path_span, size_span, hint_span]);

    let paragraph = Paragraph::new(line).block(block);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    #[test]
    fn test_render_path_prompt_with_default_name() {
        let backend = TestBackend::new(60, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_path_prompt(frame, area, "server.log", "server.log");
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);

        assert!(content.contains("Copy to: "));
        assert!(content.contains("server.log"));
        assert!(content.contains("Copy File"));
        assert!(content.contains("█"));
    }

    #[test]
    fn test_render_path_prompt_empty_input() {
        let backend = TestBackend::new(60, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_path_prompt(frame, area, "", "default.txt");
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);

        assert!(content.contains("Copy to: "));
        assert!(content.contains("█"));
        assert!(content.contains("Copy File"));
    }

    #[test]
    fn test_render_overwrite_confirm() {
        let backend = TestBackend::new(80, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        let path = Path::new("/tmp/output.log");
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_overwrite_confirm(frame, area, path);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);

        assert!(content.contains("File exists. Overwrite? (y/n):"));
        assert!(content.contains("/tmp/output.log"));
        assert!(content.contains("Copy File"));
    }

    #[test]
    fn test_render_copy_progress_bytes() {
        let backend = TestBackend::new(60, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_copy_progress(frame, area, 512);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);

        assert!(content.contains("Copying..."));
        assert!(content.contains("512 B"));
        assert!(content.contains("transferred"));
        assert!(content.contains("Copy File"));
    }

    #[test]
    fn test_render_copy_progress_kilobytes() {
        let backend = TestBackend::new(60, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_copy_progress(frame, area, 1536);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);

        assert!(content.contains("Copying..."));
        assert!(content.contains("1.5 KB"));
        assert!(content.contains("transferred"));
    }

    #[test]
    fn test_render_copy_progress_zero_bytes() {
        let backend = TestBackend::new(60, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_copy_progress(frame, area, 0);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);

        assert!(content.contains("Copying..."));
        assert!(content.contains("0 B"));
        assert!(content.contains("transferred"));
    }

    #[test]
    fn test_render_dir_copy_confirm() {
        let backend = TestBackend::new(80, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        let path = Path::new("/remote/data/logs");
        let size: u64 = 5_242_880; // 5 MB

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_dir_copy_confirm(frame, area, path, size);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);

        // Should show the remote path
        assert!(
            content.contains("/remote/data/logs"),
            "Expected path in output, got: {}",
            content
        );
        // Should show formatted size
        assert!(
            content.contains("5.0 MB"),
            "Expected formatted size in output, got: {}",
            content
        );
        // Should show key hints
        assert!(
            content.contains("[y] confirm"),
            "Expected confirm hint in output, got: {}",
            content
        );
        assert!(
            content.contains("[n/Esc] cancel"),
            "Expected cancel hint in output, got: {}",
            content
        );
        // Should show the block title
        assert!(
            content.contains("Download Directory"),
            "Expected title in output, got: {}",
            content
        );
    }

    /// Helper to extract text content from a test buffer.
    fn buffer_to_string(buf: &Buffer) -> String {
        let area = buf.area();
        let mut result = String::new();
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                let cell = &buf[(x, y)];
                result.push_str(cell.symbol());
            }
        }
        result
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
    use crate::types::format_size;

    /// Helper to extract text content from a test buffer.
    fn buffer_to_string(buf: &Buffer) -> String {
        let area = buf.area();
        let mut result = String::new();
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                let cell = &buf[(x, y)];
                result.push_str(cell.symbol());
            }
        }
        result
    }

    // Feature: arrow-nav-and-dir-download, Property 2: Confirm render content
    // Validates: Requirements 4.1
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn dir_copy_confirm_render_contains_info(
            path_str in "[a-zA-Z0-9_/]{1,40}",
            size: u64,
        ) {
            let path = Path::new(&path_str);
            let formatted_size = format_size(size);

            let backend = TestBackend::new(120, 3);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal.draw(|frame| {
                let area = frame.area();
                render_dir_copy_confirm(frame, area, path, size);
            }).unwrap();

            let buf = terminal.backend().buffer().clone();
            let content = buffer_to_string(&buf);

            prop_assert!(content.contains(&path_str),
                "Rendered content should contain path '{}', got: '{}'", path_str, content);
            prop_assert!(content.contains(&formatted_size),
                "Rendered content should contain formatted size '{}', got: '{}'", formatted_size, content);
            // Key hints
            prop_assert!(content.contains("y") || content.contains("confirm"),
                "Rendered content should mention confirm key");
            prop_assert!(content.contains("n") || content.contains("cancel") || content.contains("Esc"),
                "Rendered content should mention cancel key");
        }
    }
}
