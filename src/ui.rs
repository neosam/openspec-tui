use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use crate::runner::ImplState;

use crate::app::{App, ChangeTab, ConfigField, Screen};

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
            is_plain_text,
        } => draw_artifact_view(frame, title, content, *scroll, *is_plain_text, content_area),
        Screen::Config {
            command,
            prompt,
            cursor_position,
            focused_field,
            editing,
        } => draw_config_screen(frame, command, prompt, *cursor_position, focused_field, *editing, content_area),
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
            .title_bottom(Line::from(vec![
                Span::styled(" [C] Config ", Style::default().fg(Color::DarkGray)),
                Span::styled("[q] Quit ", Style::default().fg(Color::DarkGray)),
            ]))
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
            .title_bottom(Line::from(vec![
                Span::styled(" [C] Config ", Style::default().fg(Color::DarkGray)),
                Span::styled("[R] Run ", Style::default().fg(Color::DarkGray)),
                Span::styled("[L] Log ", Style::default().fg(Color::DarkGray)),
                Span::styled("[Esc] Back ", Style::default().fg(Color::DarkGray)),
            ]))
            .borders(Borders::ALL),
    );
    frame.render_widget(list, area);
}

pub fn draw_artifact_view(frame: &mut Frame, title: &str, content: &str, scroll: usize, is_plain_text: bool, area: Rect) {

    let text = if is_plain_text {
        ratatui::text::Text::from(content)
    } else {
        tui_markdown::from_str(content)
    };
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
                .title_bottom(Line::from(vec![
                    Span::styled(" [C] Config ", Style::default().fg(Color::DarkGray)),
                    Span::styled("[Esc] Back ", Style::default().fg(Color::DarkGray)),
                ]))
                .borders(Borders::ALL),
        );
    frame.render_widget(paragraph, area);
}

pub fn draw_config_screen(
    frame: &mut Frame,
    command: &str,
    prompt: &str,
    cursor_position: usize,
    focused_field: &ConfigField,
    editing: bool,
    area: Rect,
) {
    let chunks = Layout::vertical([
        Constraint::Length(3), // Command field
        Constraint::Min(3),   // Prompt preview
        Constraint::Length(1), // Keybinding hints
    ])
    .split(area);

    // Command field
    let cmd_style = if *focused_field == ConfigField::Command {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let cmd_block = Block::default()
        .title(" Command ")
        .borders(Borders::ALL)
        .border_style(cmd_style);

    let cmd_text = if editing && *focused_field == ConfigField::Command {
        // Show cursor only in edit mode
        let before = &command[..cursor_position];
        let cursor_char = command.get(cursor_position..cursor_position + 1).unwrap_or(" ");
        let after = if cursor_position < command.len() {
            &command[cursor_position + 1..]
        } else {
            ""
        };
        Line::from(vec![
            Span::raw(before),
            Span::styled(
                cursor_char,
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White),
            ),
            Span::raw(after),
        ])
    } else {
        Line::from(command)
    };

    let cmd_paragraph = Paragraph::new(cmd_text).block(cmd_block);
    frame.render_widget(cmd_paragraph, chunks[0]);

    // Prompt preview
    let prompt_style = if *focused_field == ConfigField::Prompt {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let prompt_title = if *focused_field == ConfigField::Prompt {
        " Prompt (Enter to edit in $EDITOR) "
    } else {
        " Prompt "
    };
    let prompt_block = Block::default()
        .title(prompt_title)
        .borders(Borders::ALL)
        .border_style(prompt_style);

    let prompt_text = prompt
        .lines()
        .map(|line| Line::from(line.to_string()))
        .collect::<Vec<_>>();
    let prompt_paragraph = Paragraph::new(prompt_text)
        .wrap(Wrap { trim: false })
        .block(prompt_block);
    frame.render_widget(prompt_paragraph, chunks[1]);

    // Warning if {prompt} is missing from command
    let has_prompt_placeholder = command.contains("{prompt}");

    // Keybinding hints
    let mut hints = if editing {
        vec![
            Span::styled(" [Esc] Done editing ", Style::default().fg(Color::DarkGray)),
        ]
    } else {
        vec![
            Span::styled(" [Enter] Edit ", Style::default().fg(Color::DarkGray)),
            Span::styled(" [Tab] Switch field ", Style::default().fg(Color::DarkGray)),
            Span::styled(" [S] Save ", Style::default().fg(Color::DarkGray)),
            Span::styled(" [D] Reset defaults ", Style::default().fg(Color::DarkGray)),
            Span::styled(" [Esc] Cancel ", Style::default().fg(Color::DarkGray)),
        ]
    };

    if !has_prompt_placeholder {
        hints.push(Span::styled(
            " ⚠ {prompt} missing in command! ",
            Style::default().fg(Color::Red),
        ));
    }

    let hints_line = Line::from(hints);
    let hints_paragraph = Paragraph::new(hints_line);
    frame.render_widget(hints_paragraph, chunks[2]);
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
        render_artifact_view_with_mode(width, height, content, scroll, false)
    }

    fn render_artifact_view_with_mode(width: u16, height: u16, content: &str, scroll: usize, is_plain_text: bool) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_artifact_view(frame, "test", content, scroll, is_plain_text, area);
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
                draw_artifact_view(frame, "test", content, 0, false, area);
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
                draw_artifact_view(frame, "test", content, 0, false, area);
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
            config: crate::config::TuiConfig::default(),
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
            config: crate::config::TuiConfig::default(),
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

    #[test]
    fn test_plain_text_preserves_single_newlines() {
        let content = "line one\nline two\nline three";
        let rendered = render_artifact_view_with_mode(40, 8, content, 0, true);
        assert!(rendered.contains("line one"), "First line should be visible");
        assert!(rendered.contains("line two"), "Second line should be visible");
        assert!(rendered.contains("line three"), "Third line should be visible");
    }

    #[test]
    fn test_plain_text_preserves_separator_lines() {
        let content = "Header\n══════════════\nContent\n──────────────";
        let rendered = render_artifact_view_with_mode(40, 8, content, 0, true);
        assert!(rendered.contains("══════"), "Double-line separator should render verbatim");
        assert!(rendered.contains("──────"), "Single-line separator should render verbatim");
    }

    #[test]
    fn test_non_log_files_use_markdown_rendering() {
        let content = "# Header\n\nBody text";
        let rendered = render_artifact_view_with_mode(40, 8, content, 0, false);
        assert!(rendered.contains("Header"), "Header text should be visible");
        assert!(rendered.contains("Body text"), "Body text should be visible");

        // Verify markdown formatting is applied (bold on header)
        let backend = TestBackend::new(40, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_artifact_view(frame, "test", content, 0, false, area);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let mut found_bold = false;
        for x in 0..40u16 {
            let cell = buffer.cell((x, 1)).unwrap();
            if cell.symbol() == "H" && cell.modifier.contains(Modifier::BOLD) {
                found_bold = true;
            }
        }
        assert!(found_bold, "Header should be bold in markdown mode");
    }

    fn render_config_screen(width: u16, height: u16, command: &str, prompt: &str, cursor_position: usize, focused_field: &ConfigField) -> String {
        render_config_screen_with_editing(width, height, command, prompt, cursor_position, focused_field, false)
    }

    fn render_config_screen_with_editing(width: u16, height: u16, command: &str, prompt: &str, cursor_position: usize, focused_field: &ConfigField, editing: bool) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_config_screen(frame, command, prompt, cursor_position, focused_field, editing, area);
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
    fn test_config_screen_shows_command() {
        let rendered = render_config_screen(60, 15, "my-tool {prompt}", "my prompt", 0, &ConfigField::Command);
        assert!(rendered.contains("my-tool"), "Command text should be visible");
    }

    #[test]
    fn test_config_screen_shows_prompt() {
        let rendered = render_config_screen(60, 15, "cmd {prompt}", "implement {name}", 0, &ConfigField::Command);
        assert!(rendered.contains("implement"), "Prompt text should be visible");
    }

    #[test]
    fn test_config_screen_shows_command_title() {
        let rendered = render_config_screen(60, 15, "cmd {prompt}", "prompt", 0, &ConfigField::Command);
        assert!(rendered.contains("Command"), "Command title should be visible");
    }

    #[test]
    fn test_config_screen_shows_prompt_title() {
        let rendered = render_config_screen(60, 15, "cmd {prompt}", "prompt", 0, &ConfigField::Command);
        assert!(rendered.contains("Prompt"), "Prompt title should be visible");
    }

    #[test]
    fn test_config_screen_shows_keybinding_hints() {
        let rendered = render_config_screen(80, 15, "cmd {prompt}", "prompt", 0, &ConfigField::Command);
        assert!(rendered.contains("[Tab]"), "Tab hint should be visible");
        assert!(rendered.contains("[S] Save"), "Save hint should be visible");
        assert!(rendered.contains("[Esc] Cancel"), "Cancel hint should be visible");
        assert!(rendered.contains("[D] Reset"), "Reset hint should be visible");
    }

    #[test]
    fn test_config_screen_warns_missing_prompt_placeholder() {
        let rendered = render_config_screen(100, 15, "cmd --flag", "prompt", 0, &ConfigField::Command);
        assert!(rendered.contains("missing"), "Warning should show when {{prompt}} is missing");
    }

    #[test]
    fn test_config_screen_no_warning_with_prompt_placeholder() {
        let rendered = render_config_screen(100, 15, "cmd {prompt}", "prompt", 0, &ConfigField::Command);
        assert!(!rendered.contains("missing"), "No warning when {{prompt}} is present");
    }

    #[test]
    fn test_config_screen_shows_editor_hint_when_prompt_focused() {
        let rendered = render_config_screen(80, 15, "cmd {prompt}", "prompt", 0, &ConfigField::Prompt);
        assert!(rendered.contains("$EDITOR"), "Editor hint should show when prompt is focused");
    }

    #[test]
    fn test_draw_full_app_with_config_screen() {
        let app = crate::app::App {
            screen: crate::app::Screen::Config {
                command: "claude --print {prompt}".to_string(),
                prompt: "implement {name}".to_string(),
                cursor_position: 0,
                focused_field: ConfigField::Command,
                editing: false,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: crate::config::TuiConfig::default(),
        };
        let rendered = render_draw(60, 15, &app);
        assert!(rendered.contains("Command"), "Config screen should render in draw()");
        assert!(rendered.contains("claude"), "Command text should be visible");
    }

    #[test]
    fn test_config_screen_no_cursor_in_navigation_mode() {
        // In navigation mode (editing=false), no block cursor should be shown
        let backend = TestBackend::new(60, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_config_screen(frame, "cmd {prompt}", "prompt", 0, &ConfigField::Command, false, area);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        // In navigation mode, no cell in the command row should have bg=White (cursor indicator)
        let mut found_cursor = false;
        for x in 0..60u16 {
            let cell = buffer.cell((x, 1)).unwrap(); // row 1 is inside the Command block
            if cell.bg == Color::White {
                found_cursor = true;
            }
        }
        assert!(!found_cursor, "No cursor should be visible in navigation mode");
    }

    #[test]
    fn test_config_screen_cursor_visible_in_edit_mode() {
        let backend = TestBackend::new(60, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_config_screen(frame, "cmd {prompt}", "prompt", 0, &ConfigField::Command, true, area);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        // In edit mode, a cell in the command row should have bg=White (cursor indicator)
        let mut found_cursor = false;
        for x in 0..60u16 {
            let cell = buffer.cell((x, 1)).unwrap();
            if cell.bg == Color::White {
                found_cursor = true;
            }
        }
        assert!(found_cursor, "Cursor should be visible in edit mode");
    }

    #[test]
    fn test_config_screen_navigation_hints() {
        let rendered = render_config_screen_with_editing(
            100, 15, "cmd {prompt}", "prompt", 0, &ConfigField::Command, false,
        );
        assert!(rendered.contains("[Enter] Edit"), "Enter hint should show in navigation mode");
        assert!(rendered.contains("[Tab]"), "Tab hint should show in navigation mode");
        assert!(rendered.contains("[S] Save"), "Save hint should show in navigation mode");
        assert!(rendered.contains("[D] Reset"), "Reset hint should show in navigation mode");
        assert!(rendered.contains("[Esc] Cancel"), "Cancel hint should show in navigation mode");
        assert!(!rendered.contains("Done editing"), "Edit-mode hint should not show in navigation mode");
    }

    #[test]
    fn test_config_screen_edit_mode_hints() {
        let rendered = render_config_screen_with_editing(
            100, 15, "cmd {prompt}", "prompt", 0, &ConfigField::Command, true,
        );
        assert!(rendered.contains("[Esc] Done editing"), "Done editing hint should show in edit mode");
        assert!(!rendered.contains("[S] Save"), "Save hint should not show in edit mode");
        assert!(!rendered.contains("[D] Reset"), "Reset hint should not show in edit mode");
        assert!(!rendered.contains("[Enter] Edit"), "Enter-edit hint should not show in edit mode");
    }

    #[test]
    fn test_config_screen_prompt_warning_in_both_modes() {
        // Navigation mode without {prompt}
        let rendered_nav = render_config_screen_with_editing(
            100, 15, "cmd --flag", "prompt", 0, &ConfigField::Command, false,
        );
        assert!(rendered_nav.contains("missing"), "Warning should show in navigation mode");

        // Edit mode without {prompt}
        let rendered_edit = render_config_screen_with_editing(
            100, 15, "cmd --flag", "prompt", 0, &ConfigField::Command, true,
        );
        assert!(rendered_edit.contains("missing"), "Warning should show in edit mode");
    }
}
