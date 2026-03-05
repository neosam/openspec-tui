use ratatui::{
    Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use crate::app::{App, Screen};

pub fn draw(frame: &mut Frame, app: &App) {
    match &app.screen {
        Screen::ChangeList {
            changes,
            selected,
            error,
        } => draw_change_list(frame, changes, *selected, error.as_deref()),
        Screen::ArtifactMenu {
            change_name,
            items,
            selected,
            ..
        } => draw_artifact_menu(frame, change_name, items, *selected),
        Screen::ArtifactView {
            title,
            content,
            scroll,
        } => draw_artifact_view(frame, title, content, *scroll),
    }
}

fn draw_change_list(
    frame: &mut Frame,
    changes: &[crate::data::ChangeEntry],
    selected: usize,
    error: Option<&str>,
) {
    let area = frame.area();

    if let Some(err) = error {
        let paragraph = Paragraph::new(err)
            .style(Style::default().fg(Color::Red))
            .block(
                Block::default()
                    .title(" OpenSpec TUI - Error ")
                    .borders(Borders::ALL),
            );
        frame.render_widget(paragraph, area);
        return;
    }

    if changes.is_empty() {
        let paragraph = Paragraph::new("No active changes found.")
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .title(" OpenSpec TUI ")
                    .borders(Borders::ALL),
            );
        frame.render_widget(paragraph, area);
        return;
    }

    let items: Vec<ListItem> = changes
        .iter()
        .enumerate()
        .map(|(i, change)| {
            let style = if i == selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let line = Line::from(vec![
                Span::styled(&change.name, style),
                Span::styled(
                    format!("  ({}/{})", change.completed_tasks, change.total_tasks),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" OpenSpec TUI - Changes ")
            .borders(Borders::ALL),
    );
    frame.render_widget(list, area);
}

fn draw_artifact_menu(
    frame: &mut Frame,
    change_name: &str,
    items: &[crate::app::ArtifactMenuItem],
    selected: usize,
) {
    let area = frame.area();

    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let style = if !item.available && !item.is_spec_header {
                Style::default().fg(Color::DarkGray)
            } else if item.is_spec_header && !item.available {
                Style::default().fg(Color::DarkGray)
            } else if i == selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let indicator = if i == selected { "> " } else { "  " };
            ListItem::new(Line::from(Span::styled(
                format!("{}{}", indicator, item.label),
                style,
            )))
        })
        .collect();

    let list = List::new(list_items).block(
        Block::default()
            .title(format!(" {} - Artifacts ", change_name))
            .borders(Borders::ALL),
    );
    frame.render_widget(list, area);
}

pub fn draw_artifact_view(frame: &mut Frame, title: &str, content: &str, scroll: usize) {
    let area = frame.area();

    let lines: Vec<Line> = content.lines().map(Line::from).collect();
    let total_lines = lines.len();

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0))
        .block(
            Block::default()
                .title(format!(
                    " {} [{}/{}] ",
                    title,
                    scroll + 1,
                    total_lines
                ))
                .borders(Borders::ALL),
        );
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn render_artifact_view(width: u16, height: u16, content: &str, scroll: usize) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                draw_artifact_view(frame, "test", content, scroll);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let mut lines = Vec::new();
        for y in 0..height {
            let mut line = String::new();
            for x in 0..width {
                line.push_str(buffer.cell((x, y)).unwrap().symbol());
            }
            lines.push(line.trim_end().to_string());
        }
        lines.join("\n")
    }

    #[test]
    fn test_long_lines_are_wrapped() {
        // Terminal is 20 wide; border takes 2 chars, leaving 18 content chars.
        // A line longer than 18 chars should wrap onto the next rendered line.
        let content = "aaa bbb ccc ddd eee fff ggg";
        let rendered = render_artifact_view(20, 6, content, 0);
        // The content area is lines 1..5 (between top and bottom border).
        // With wrapping, the long line should span multiple rendered lines.
        // With wrapping enabled, the long line should span multiple rendered lines.
        // Verify that both early and later words appear in the render.
        assert!(rendered.contains("aaa"), "First word should be visible");
        assert!(rendered.contains("fff"), "Later word should be visible via wrapping");
    }

    #[test]
    fn test_short_lines_remain_unchanged() {
        let content = "short line";
        let rendered = render_artifact_view(40, 5, content, 0);
        let content_lines: Vec<&str> = rendered.lines().collect();
        // Content line 1 (after top border) should contain the text
        assert!(content_lines[1].contains("short line"));
        // Content line 2 should be empty (no wrapping occurred)
        let line2_content = content_lines[2]
            .trim_start_matches('│')
            .trim_end_matches('│')
            .trim();
        assert!(line2_content.is_empty(), "Second content line should be empty for short text");
    }

    #[test]
    fn test_leading_whitespace_preserved() {
        let content = "    indented text that is quite long and should wrap";
        let rendered = render_artifact_view(30, 6, content, 0);
        let content_lines: Vec<&str> = rendered.lines().collect();
        // The first content line (after border) should start with the leading spaces
        let first_content = content_lines[1]
            .trim_start_matches('│');
        assert!(first_content.starts_with("    "), "Leading whitespace should be preserved, got: '{}'", first_content);
    }
}
