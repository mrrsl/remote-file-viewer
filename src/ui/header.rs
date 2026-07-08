// Header bar widget: current path and connection info

use std::path::Path;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// Render the header bar showing the current absolute path (left) and connection info (right).
///
/// The header is a single-line styled bar with:
/// - Left side: current absolute path in bold white
/// - Right side: "username@host" in a dimmed style
/// - Background color to visually distinguish it as a header
pub fn render_header(
    frame: &mut Frame,
    area: Rect,
    current_path: &Path,
    username: &str,
    host: &str,
) {
    let path_str = current_path.to_string_lossy();
    let connection_info = format!("{}@{}", username, host);

    // Calculate available width for path, leaving space for connection info + padding
    let total_width = area.width as usize;
    let connection_len = connection_info.len();

    // We need at least 1 space separator between path and connection info
    let path_max_width = total_width.saturating_sub(connection_len + 1);

    // Truncate path if it exceeds the available space
    let displayed_path = if path_str.len() > path_max_width && path_max_width > 3 {
        format!("...{}", &path_str[path_str.len() - (path_max_width - 3)..])
    } else if path_max_width == 0 {
        String::new()
    } else {
        path_str.to_string()
    };

    // Calculate padding between path and connection info to right-align connection
    let padding_len = total_width
        .saturating_sub(displayed_path.len() + connection_len);

    let padding = " ".repeat(padding_len);

    let header_style = Style::default().bg(Color::DarkGray);

    let line = Line::from(vec![
        Span::styled(
            displayed_path,
            header_style.add_modifier(Modifier::BOLD).fg(Color::White),
        ),
        Span::styled(padding, header_style),
        Span::styled(
            connection_info,
            header_style.fg(Color::Gray),
        ),
    ]);

    let paragraph = Paragraph::new(line).style(header_style);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    #[test]
    fn test_render_header_basic() {
        let backend = TestBackend::new(40, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                let path = PathBuf::from("/home/user/documents");
                render_header(frame, area, &path, "deploy", "192.168.1.1");
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);

        // Verify path is present
        assert!(content.contains("/home/user/documents"));
        // Verify connection info is present
        assert!(content.contains("deploy@192.168.1.1"));
    }

    #[test]
    fn test_render_header_long_path_truncated() {
        let backend = TestBackend::new(40, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                let path = PathBuf::from("/very/long/path/that/exceeds/available/width/in/terminal");
                render_header(frame, area, &path, "user", "host");
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);

        // Connection info should still be visible
        assert!(content.contains("user@host"));
        // Path should be truncated (starts with "...")
        assert!(content.contains("..."));
    }

    #[test]
    fn test_render_header_root_path() {
        let backend = TestBackend::new(30, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                let path = PathBuf::from("/");
                render_header(frame, area, &path, "root", "server");
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);

        assert!(content.contains("/"));
        assert!(content.contains("root@server"));
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
