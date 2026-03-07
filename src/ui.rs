use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use crate::runner::ImplState;

use crate::app::{App, ChangeTab, Screen};

pub fn draw(frame: &mut Frame, app: &App) {
    let (content_area, status_area) = if let Some(ref impl_state) = app.implementation {
        let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(4)]).split(frame.area());
        draw_status_bar(frame, impl_state, chunks[1]);
        (chunks[0], Some(chunks[1]))
    } else {
        (frame.area(), None)
    };
    let _ = status_area;

    match &app.screen {
        Screen::ChangeList {
            changes,
            selected,
            error,
            tab,
        } => draw_change_list(frame, changes, *selected, error.as_deref(), tab, content_area),
        Screen::ArtifactMenu {
            change_name,
            items,
            selected,
            ..
        } => draw_artifact_menu(frame, change_name, items, *selected, content_area),
        Screen::ArtifactView {
            title,
            content,
            scroll,
        } => draw_artifact_view(frame, title, content, *scroll, content_area),
    }
}

fn tab_title(tab: &ChangeTab) -> Line<'static> {
    let (active_style, archived_style) = match tab {
        ChangeTab::Active => (
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(Color::DarkGray),
        ),
        ChangeTab::Archived => (
            Style::default().fg(Color::DarkGray),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    };

    Line::from(vec![
        Span::raw(" OpenSpec TUI ["),
        Span::styled("Active", active_style),
        Span::raw(" | "),
        Span::styled("Archived", archived_style),
        Span::raw("] "),
    ])
}

fn draw_change_list(
    frame: &mut Frame,
    changes: &[crate::data::ChangeEntry],
    selected: usize,
    error: Option<&str>,
    tab: &ChangeTab,
    area: Rect,
) {
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

    let title = tab_title(tab);

    if changes.is_empty() {
        let empty_msg = match tab {
            ChangeTab::Active => "No active changes found.",
            ChangeTab::Archived => "No archived changes found.",
        };
        let paragraph = Paragraph::new(empty_msg)
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .title(title)
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
            .title(title)
            .borders(Borders::ALL),
    );
    frame.render_widget(list, area);
}

fn draw_artifact_menu(
    frame: &mut Frame,
    change_name: &str,
    items: &[crate::app::ArtifactMenuItem],
    selected: usize,
    area: Rect,
) {

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

pub fn draw_artifact_view(frame: &mut Frame, title: &str, content: &str, scroll: usize, area: Rect) {

    let text = tui_markdown::from_str(content);
    let total_lines = text.lines.len();

    let paragraph = Paragraph::new(text)
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

pub fn draw_status_bar(frame: &mut Frame, impl_state: &ImplState, area: Rect) {
    let progress_pct = if impl_state.total > 0 {
        (impl_state.completed as f64 / impl_state.total as f64 * 100.0) as u16
    } else {
        0
    };

    // Build progress bar: use the available inner width minus the text portions
    // Line 1: ⟳ change-name  completed/total  [████░░] pct%
    // Line 2: Log: /path/to/log  [S] Stop
    let bar_width = area.width.saturating_sub(2) as usize; // account for borders
    let prefix = format!(
        " ⟳ {}  {}/{}  ",
        impl_state.change_name, impl_state.completed, impl_state.total
    );
    let suffix = format!(" {}%", progress_pct);
    let bar_space = bar_width
        .saturating_sub(prefix.len())
        .saturating_sub(suffix.len());
    let filled = if impl_state.total > 0 {
        (bar_space as u32 * impl_state.completed / impl_state.total) as usize
    } else {
        0
    };
    let empty = bar_space.saturating_sub(filled);

    let line1 = Line::from(vec![
        Span::styled(&prefix, Style::default().fg(Color::Cyan)),
        Span::styled("█".repeat(filled), Style::default().fg(Color::Green)),
        Span::styled("░".repeat(empty), Style::default().fg(Color::DarkGray)),
        Span::styled(&suffix, Style::default().fg(Color::Cyan)),
    ]);

    let log_display = impl_state.log_path.display().to_string();
    let line2 = Line::from(vec![
        Span::styled(
            format!(" Log: {}", log_display),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled("  [S] Stop", Style::default().fg(Color::Yellow)),
    ]);

    let paragraph = Paragraph::new(vec![line1, line2]).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    fn render_artifact_view(width: u16, height: u16, content: &str, scroll: usize) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_artifact_view(frame, "test", content, scroll, area);
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
    fn test_markdown_headers_rendered_as_formatted_text() {
        let content = "# Main Header\n\nSome body text.";
        let rendered = render_artifact_view(40, 8, content, 0);
        // Header text and body text should both be visible
        assert!(rendered.contains("Main Header"), "Header text should be visible");
        assert!(rendered.contains("Some body text"), "Body text should be visible");

        // Verify header has bold styling applied
        let backend = TestBackend::new(40, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_artifact_view(frame, "test", content, 0, area);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        // Find a cell in the header line and check it has bold modifier
        let mut found_bold = false;
        for x in 0..40u16 {
            let cell = buffer.cell((x, 1)).unwrap();
            if cell.symbol() == "M" {
                if cell.modifier.contains(Modifier::BOLD) {
                    found_bold = true;
                }
            }
        }
        assert!(found_bold, "Header text should be rendered with bold styling");
    }

    #[test]
    fn test_code_blocks_rendered_with_highlighting() {
        let content = "```rust\nfn main() {}\n```";
        let rendered = render_artifact_view(40, 8, content, 0);
        // Code content should be visible
        assert!(rendered.contains("fn"), "Code keyword should be visible");
        assert!(rendered.contains("main"), "Function name should be visible");

        // Verify syntax highlighting: check that the code content cell has non-default styling
        let backend = TestBackend::new(40, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_artifact_view(frame, "test", content, 0, area);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        // Find a cell containing "fn" and check it has a foreground color set (syntax highlighting)
        let mut found_styled = false;
        for y in 0..8u16 {
            for x in 0..40u16 {
                let cell = buffer.cell((x, y)).unwrap();
                if cell.symbol() == "f" && x + 1 < 40 && buffer.cell((x + 1, y)).unwrap().symbol() == "n" {
                    // "fn" keyword found; check if it has non-default foreground color
                    if cell.fg != Color::Reset && cell.fg != Color::default() {
                        found_styled = true;
                    }
                }
            }
        }
        assert!(found_styled, "Code keyword 'fn' should have syntax highlighting (non-default color)");
    }

    #[test]
    fn test_indented_code_block_rendered() {
        // 4-space indented text is rendered as a code block in Markdown
        let content = "    indented code";
        let rendered = render_artifact_view(30, 6, content, 0);
        // The indented text should still be visible in the rendered output
        assert!(rendered.contains("indented code"), "Code block content should be visible");
    }

    fn render_status_bar(width: u16, height: u16, impl_state: &ImplState) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = Rect::new(0, 0, width, height);
                draw_status_bar(frame, impl_state, area);
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

    fn make_impl_state(
        change_name: &str,
        completed: u32,
        total: u32,
    ) -> ImplState {
        use std::sync::atomic::AtomicBool;
        use std::sync::{mpsc, Arc, Mutex};
        use std::path::PathBuf;

        let (_tx, rx) = mpsc::channel();
        ImplState {
            change_name: change_name.to_string(),
            completed,
            total,
            log_path: PathBuf::from(format!("openspec/changes/{}/implementation.log", change_name)),
            receiver: rx,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            child_handle: Arc::new(Mutex::new(None)),
        }
    }

    #[test]
    fn test_status_bar_shows_change_name() {
        let state = make_impl_state("my-change", 3, 7);
        let rendered = render_status_bar(60, 4, &state);
        assert!(rendered.contains("my-change"), "Change name should be displayed");
    }

    #[test]
    fn test_status_bar_shows_task_counts() {
        let state = make_impl_state("test", 3, 7);
        let rendered = render_status_bar(60, 4, &state);
        assert!(rendered.contains("3/7"), "Task counts should be displayed");
    }

    #[test]
    fn test_status_bar_shows_progress_bar() {
        let state = make_impl_state("test", 5, 10);
        let rendered = render_status_bar(60, 4, &state);
        assert!(rendered.contains("█"), "Progress bar should have filled blocks");
        assert!(rendered.contains("░"), "Progress bar should have empty blocks");
        assert!(rendered.contains("50%"), "Percentage should be displayed");
    }

    #[test]
    fn test_status_bar_shows_stop_hint() {
        let state = make_impl_state("test", 0, 5);
        let rendered = render_status_bar(60, 4, &state);
        assert!(rendered.contains("[S] Stop"), "Stop hint should be displayed");
    }

    #[test]
    fn test_status_bar_shows_log_path() {
        let state = make_impl_state("test", 0, 5);
        let rendered = render_status_bar(80, 4, &state);
        assert!(rendered.contains("Log:"), "Log path label should be displayed");
        assert!(
            rendered.contains("openspec/changes/test/implementation.log"),
            "Log path should show the change-local path"
        );
    }

    #[test]
    fn test_status_bar_zero_total() {
        let state = make_impl_state("test", 0, 0);
        let rendered = render_status_bar(60, 4, &state);
        assert!(rendered.contains("0/0"), "Zero progress should be displayed");
        assert!(rendered.contains("0%"), "Zero percentage should be displayed");
    }

    #[test]
    fn test_status_bar_all_complete() {
        let state = make_impl_state("test", 5, 5);
        let rendered = render_status_bar(60, 4, &state);
        assert!(rendered.contains("5/5"), "Complete progress should be displayed");
        assert!(rendered.contains("100%"), "100% should be displayed");
    }

    fn render_draw(width: u16, height: u16, app: &crate::app::App) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                draw(frame, app);
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
    fn test_layout_split_when_implementation_running() {
        let app = crate::app::App {
            screen: crate::app::Screen::ChangeList {
                changes: vec![],
                selected: 0,
                error: None,
                tab: crate::app::ChangeTab::Active,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: Some(make_impl_state("my-change", 2, 5)),
        };

        let rendered = render_draw(80, 14, &app);
        // Status bar should be visible at the bottom
        assert!(rendered.contains("my-change"), "Status bar should show change name");
        assert!(rendered.contains("2/5"), "Status bar should show progress");
        assert!(rendered.contains("[S] Stop"), "Status bar should show stop hint");
        // Main content should also be visible
        assert!(
            rendered.contains("OpenSpec TUI"),
            "Main content header should be visible"
        );
    }

    #[test]
    fn test_layout_not_split_when_no_implementation() {
        let app = crate::app::App {
            screen: crate::app::Screen::ChangeList {
                changes: vec![],
                selected: 0,
                error: None,
                tab: crate::app::ChangeTab::Active,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
        };

        let rendered = render_draw(60, 14, &app);
        // No status bar content should be present
        assert!(
            !rendered.contains("[S] Stop"),
            "Stop hint should not appear without implementation"
        );
        // Main content should use the full area
        assert!(
            rendered.contains("OpenSpec TUI"),
            "Main content header should be visible"
        );
    }

    fn render_change_list(width: u16, height: u16, tab: &crate::app::ChangeTab) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_change_list(
                    frame,
                    &[],
                    0,
                    None,
                    tab,
                    area,
                );
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
    fn test_title_rendering_active_tab() {
        let rendered = render_change_list(50, 5, &crate::app::ChangeTab::Active);
        assert!(
            rendered.contains("OpenSpec TUI"),
            "Title should contain OpenSpec TUI"
        );
        assert!(
            rendered.contains("Active"),
            "Title should contain Active tab label"
        );
        assert!(
            rendered.contains("Archived"),
            "Title should contain Archived tab label"
        );
    }

    #[test]
    fn test_title_rendering_archived_tab() {
        let rendered = render_change_list(50, 5, &crate::app::ChangeTab::Archived);
        assert!(
            rendered.contains("OpenSpec TUI"),
            "Title should contain OpenSpec TUI"
        );
        assert!(
            rendered.contains("Active"),
            "Title should contain Active tab label"
        );
        assert!(
            rendered.contains("Archived"),
            "Title should contain Archived tab label"
        );
    }

    #[test]
    fn test_empty_message_active_tab() {
        let rendered = render_change_list(50, 5, &crate::app::ChangeTab::Active);
        assert!(
            rendered.contains("No active changes found"),
            "Should show active-specific empty message"
        );
    }

    #[test]
    fn test_empty_message_archived_tab() {
        let rendered = render_change_list(50, 5, &crate::app::ChangeTab::Archived);
        assert!(
            rendered.contains("No archived changes found"),
            "Should show archived-specific empty message"
        );
    }
}
