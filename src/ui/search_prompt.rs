// Search input widget

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::SearchType;

/// Render the search prompt widget.
///
/// - `query`: current input text typed by the user
/// - `search_type`: Local or Global (determines label)
/// - `progress`: if Some(n), show "Searching... (n directories traversed)" for global find in progress
pub fn render_search_prompt(
    frame: &mut Frame,
    area: Rect,
    query: &str,
    search_type: &SearchType,
    progress: Option<usize>,
) {
    let label = match search_type {
        SearchType::Local => "Find (local): ",
        SearchType::Global => "Find (global): ",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Search ");

    if let Some(dirs_traversed) = progress {
        // Show progress indicator instead of the input field
        let progress_text = format!(
            "{}Searching... ({} directories traversed)",
            label, dirs_traversed
        );
        let line = Line::from(vec![
            Span::styled(
                progress_text,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]);
        let paragraph = Paragraph::new(line).block(block);
        frame.render_widget(paragraph, area);
    } else {
        // Show the input field with label and cursor
        let cursor_char = "█";
        let line = Line::from(vec![
            Span::styled(
                label.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(query.to_string(), Style::default().fg(Color::White)),
            Span::styled(
                cursor_char.to_string(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ]);
        let paragraph = Paragraph::new(line).block(block);
        frame.render_widget(paragraph, area);
    }
}
