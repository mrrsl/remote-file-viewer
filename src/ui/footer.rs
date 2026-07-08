// Footer bar / status widget

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::types::{StatusLevel, StatusMessage};

/// Render the footer bar. Shows status messages when active, a loading indicator
/// during operations, or the default key bindings.
pub fn render_footer(
    frame: &mut Frame,
    area: Rect,
    status_message: Option<&StatusMessage>,
    loading: bool,
) {
    let paragraph = if loading {
        let line = Line::from(vec![Span::styled(
            "⏳ Loading...",
            Style::default().fg(Color::Yellow),
        )]);
        Paragraph::new(line)
    } else if let Some(msg) = status_message {
        let color = match msg.level {
            StatusLevel::Info => Color::Yellow,
            StatusLevel::Success => Color::Green,
            StatusLevel::Error => Color::Red,
        };
        let line = Line::from(vec![Span::styled(
            msg.text.clone(),
            Style::default().fg(color),
        )]);
        Paragraph::new(line)
    } else {
        let line = Line::from(vec![
            Span::styled("q", Style::default().fg(Color::Cyan)),
            Span::raw(": quit  "),
            Span::styled("o", Style::default().fg(Color::Cyan)),
            Span::raw(": open  "),
            Span::styled("c", Style::default().fg(Color::Cyan)),
            Span::raw(": copy  "),
            Span::styled("^F", Style::default().fg(Color::Cyan)),
            Span::raw(": find  "),
            Span::styled("^G", Style::default().fg(Color::Cyan)),
            Span::raw(": global find  "),
            Span::styled("m", Style::default().fg(Color::Cyan)),
            Span::raw(": navigate  "),
            Span::styled("a", Style::default().fg(Color::Cyan)),
            Span::raw(": toggle hidden"),
        ]);
        Paragraph::new(line)
    };

    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    use std::time::Instant;

    #[test]
    fn test_render_footer_default_key_bindings() {
        let backend = TestBackend::new(100, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_footer(frame, area, None, false);
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content: String = (0..buffer.area.width)
            .map(|x| buffer[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();

        assert!(content.contains("q"));
        assert!(content.contains("quit"));
        assert!(content.contains("o"));
        assert!(content.contains("open"));
        assert!(content.contains("c"));
        assert!(content.contains("copy"));
        assert!(content.contains("find"));
        assert!(content.contains("global find"));
        assert!(content.contains("m"));
        assert!(content.contains("navigate"));
        assert!(content.contains("a"));
        assert!(content.contains("toggle hidden"));
    }

    #[test]
    fn test_render_footer_loading() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_footer(frame, area, None, true);
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content: String = (0..buffer.area.width)
            .map(|x| buffer[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();

        assert!(content.contains("Loading"));
    }

    #[test]
    fn test_render_footer_status_info() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        let msg = StatusMessage {
            text: "Listing directory...".to_string(),
            level: StatusLevel::Info,
            created_at: Instant::now(),
        };

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_footer(frame, area, Some(&msg), false);
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content: String = (0..buffer.area.width)
            .map(|x| buffer[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();

        assert!(content.contains("Listing directory..."));
        // Key bindings should NOT appear when a status message is shown
        assert!(!content.contains("quit"));
    }

    #[test]
    fn test_render_footer_status_success() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        let msg = StatusMessage {
            text: "File copied to ./output.log".to_string(),
            level: StatusLevel::Success,
            created_at: Instant::now(),
        };

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_footer(frame, area, Some(&msg), false);
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content: String = (0..buffer.area.width)
            .map(|x| buffer[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();

        assert!(content.contains("File copied to ./output.log"));
    }

    #[test]
    fn test_render_footer_status_error() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        let msg = StatusMessage {
            text: "Permission denied".to_string(),
            level: StatusLevel::Error,
            created_at: Instant::now(),
        };

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_footer(frame, area, Some(&msg), false);
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content: String = (0..buffer.area.width)
            .map(|x| buffer[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();

        assert!(content.contains("Permission denied"));
    }

    #[test]
    fn test_render_footer_loading_takes_priority_over_status() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        let msg = StatusMessage {
            text: "Some status".to_string(),
            level: StatusLevel::Info,
            created_at: Instant::now(),
        };

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_footer(frame, area, Some(&msg), true);
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content: String = (0..buffer.area.width)
            .map(|x| buffer[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();

        // Loading indicator should be shown, not the status message
        assert!(content.contains("Loading"));
        assert!(!content.contains("Some status"));
    }
}
