// File browser list widget

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::types::{DirectoryEntry, EntryType, format_size, truncate_name};

/// Render the file browser widget displaying directory entries with selection highlighting.
///
/// If `entries` is empty, displays a centered "Empty directory" message.
/// Otherwise, renders each entry with a type indicator, truncated name, and size.
/// The selected entry is highlighted and the viewport scrolls to keep it visible.
pub fn render_browser(
    frame: &mut Frame,
    area: Rect,
    entries: &[DirectoryEntry],
    selected_index: usize,
) {
    let block = Block::default().borders(Borders::NONE);

    if entries.is_empty() {
        let message = Paragraph::new("Empty directory")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(message, area);
        return;
    }

    let items: Vec<ListItem> = entries
        .iter()
        .map(|entry| {
            let type_indicator = match entry.entry_type {
                EntryType::Directory => "d ",
                EntryType::Symlink => "@ ",
                EntryType::File => "  ",
            };

            // Calculate available width for the name:
            // area.width - highlight_symbol(2) - type_indicator(2) - size field - padding(2)
            let size_str = format_size(entry.size);
            let highlight_width = 2; // "▶ " symbol
            let reserved = highlight_width + 2 + size_str.len() + 2; // highlight + type_indicator + size + spacing
            let name_max = if area.width as usize > reserved {
                area.width as usize - reserved
            } else {
                10 // minimum fallback
            };
            let truncated_name = truncate_name(&entry.name, name_max);

            // Pad the name to fill available space so size aligns to the right
            let name_display_width = if area.width as usize > reserved {
                area.width as usize - reserved
            } else {
                truncated_name.len()
            };
            let padded_name = format!("{:<width$}", truncated_name, width = name_display_width);

            let line = Line::from(vec![
                Span::styled(
                    type_indicator.to_string(),
                    Style::default().fg(match entry.entry_type {
                        EntryType::Directory => Color::Blue,
                        EntryType::Symlink => Color::Cyan,
                        EntryType::File => Color::White,
                    }),
                ),
                Span::raw(padded_name),
                Span::styled(
                    format!("  {}", size_str),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    // Use ListState to track selection and handle scrolling
    let mut state = ListState::default();
    state.select(Some(selected_index));

    frame.render_stateful_widget(list, area, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    use std::path::PathBuf;

    fn make_entry(name: &str, entry_type: EntryType, size: u64) -> DirectoryEntry {
        DirectoryEntry {
            name: name.to_string(),
            path: PathBuf::from(format!("/test/{}", name)),
            entry_type,
            size,
        }
    }

    #[test]
    fn test_render_empty_directory() {
        let backend = TestBackend::new(40, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_browser(frame, area, &[], 0);
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        // Scan all rows for the empty directory message
        let mut found = false;
        for y in 0..buffer.area.height {
            let row: String = (0..buffer.area.width)
                .map(|x| buffer.cell((x, y)).unwrap().symbol().to_string())
                .collect::<String>();
            if row.contains("Empty directory") {
                found = true;
                break;
            }
        }
        assert!(found, "Expected 'Empty directory' somewhere in the buffer");
    }

    #[test]
    fn test_render_entries_with_types() {
        let entries = vec![
            make_entry("docs", EntryType::Directory, 0),
            make_entry("readme.md", EntryType::File, 1024),
            make_entry("link", EntryType::Symlink, 0),
        ];

        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_browser(frame, area, &entries, 0);
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();

        // Check that directory entry has "d " prefix
        let first_row: String = (0..buffer.area.width)
            .map(|x| buffer.cell((x, 0)).unwrap().symbol().to_string())
            .collect::<String>();
        assert!(
            first_row.contains("d ") && first_row.contains("docs"),
            "Expected directory indicator and name, got: {:?}",
            first_row
        );

        // Check that file entry has "  " prefix (two spaces)
        let second_row: String = (0..buffer.area.width)
            .map(|x| buffer.cell((x, 1)).unwrap().symbol().to_string())
            .collect::<String>();
        assert!(
            second_row.contains("readme.md"),
            "Expected file name, got: {:?}",
            second_row
        );

        // Check that symlink entry has "@ " prefix
        let third_row: String = (0..buffer.area.width)
            .map(|x| buffer.cell((x, 2)).unwrap().symbol().to_string())
            .collect::<String>();
        assert!(
            third_row.contains("@ ") && third_row.contains("link"),
            "Expected symlink indicator and name, got: {:?}",
            third_row
        );
    }

    #[test]
    fn test_render_shows_size() {
        let entries = vec![make_entry("big_file.log", EntryType::File, 2_097_152)];

        let backend = TestBackend::new(80, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_browser(frame, area, &entries, 0);
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let row: String = (0..buffer.area.width)
            .map(|x| buffer.cell((x, 0)).unwrap().symbol().to_string())
            .collect::<String>();
        assert!(
            row.contains("2.0 MB"),
            "Expected size '2.0 MB', got: {:?}",
            row
        );
    }

    #[test]
    fn test_render_scrolling_selected_visible() {
        // Create more entries than the viewport can display
        let entries: Vec<DirectoryEntry> = (0..20)
            .map(|i| make_entry(&format!("file_{:02}.txt", i), EntryType::File, i * 100))
            .collect();

        let backend = TestBackend::new(60, 5); // Only 5 rows visible
        let mut terminal = Terminal::new(backend).unwrap();

        // Select an entry beyond the visible area
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_browser(frame, area, &entries, 15);
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        // The selected entry (file_15.txt) should be visible somewhere in the buffer
        let mut all_content = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                all_content.push_str(buffer.cell((x, y)).unwrap().symbol());
            }
        }
        assert!(
            all_content.contains("file_15"),
            "Expected selected entry 'file_15' to be visible, got: {:?}",
            all_content
        );
    }
}
