use ratatui::{
    Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
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

fn draw_artifact_view(frame: &mut Frame, title: &str, content: &str, scroll: usize) {
    let area = frame.area();

    let lines: Vec<Line> = content.lines().map(Line::from).collect();
    let total_lines = lines.len();

    let paragraph = Paragraph::new(lines)
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
