// Direct path navigation input widget

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

/// Render the navigate prompt widget for direct path entry.
///
/// Displays a bordered input area with:
/// - A "Navigate to: " label in bold
/// - The current input text
/// - A block cursor indicator (▌) at the end of the input
pub fn render_navigate_prompt(frame: &mut Frame, area: Rect, input: &str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Go To Path ");

    let label = Span::styled(
        "Navigate to: ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let input_text = Span::styled(input, Style::default().fg(Color::White));

    let cursor = Span::styled("▌", Style::default().fg(Color::Cyan));

    let line = Line::from(vec![label, input_text, cursor]);

    let paragraph = Paragraph::new(line).block(block);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    #[test]
    fn test_render_navigate_prompt_empty_input() {
        let backend = TestBackend::new(50, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_navigate_prompt(frame, area, "");
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);

        assert!(content.contains("Navigate to: "));
        assert!(content.contains("▌"));
        assert!(content.contains("Go To Path"));
    }

    #[test]
    fn test_render_navigate_prompt_with_input() {
        let backend = TestBackend::new(60, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_navigate_prompt(frame, area, "/var/log/kafka");
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);

        assert!(content.contains("Navigate to: "));
        assert!(content.contains("/var/log/kafka"));
        assert!(content.contains("▌"));
    }

    #[test]
    fn test_render_navigate_prompt_long_path() {
        let backend = TestBackend::new(80, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        let long_path = "/home/user/very/deep/nested/directory/structure/path";
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_navigate_prompt(frame, area, long_path);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let content = buffer_to_string(&buf);

        assert!(content.contains("Navigate to: "));
        // At least part of the path should be visible
        assert!(content.contains("/home/user"));
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
