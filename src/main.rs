mod app;
mod config;
mod data;
mod runner;
mod ui;

use std::io;

use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::CrosstermBackend;

use app::{App, Screen};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Install panic hook to restore terminal on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let result = run_app(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    if let Err(err) = result {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }

    Ok(())
}

/// Suspend the TUI, open `$EDITOR` (fallback: `vi`) with a temp file containing `content`,
/// and return the edited text.
fn edit_in_external_editor(
    terminal: &mut ratatui::Terminal<CrosstermBackend<io::Stdout>>,
    content: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    use std::io::Write as _;

    let mut tmp = tempfile::NamedTempFile::new()?;
    tmp.write_all(content.as_bytes())?;
    tmp.flush()?;
    let path = tmp.path().to_path_buf();

    // Restore terminal for the editor
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let status = std::process::Command::new(&editor)
        .arg(&path)
        .status();

    // Re-enter TUI mode
    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.clear()?;

    status?;
    let edited = std::fs::read_to_string(&path)?;
    Ok(edited)
}

fn run_app(terminal: &mut ratatui::Terminal<CrosstermBackend<io::Stdout>>) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new()?;

    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        app.poll_implementation();

        if event::poll(Duration::from_millis(500))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                // In Config screen, all keys go to the config handler
                if matches!(app.screen, Screen::Config { .. }) {
                    let open_editor = app.handle_config_input(key.code);
                    if open_editor {
                        // Get current prompt text
                        let current_prompt = if let Screen::Config { prompt, .. } = &app.screen {
                            prompt.clone()
                        } else {
                            String::new()
                        };
                        // Open $EDITOR with temp file
                        if let Ok(new_prompt) = edit_in_external_editor(terminal, &current_prompt) {
                            app.set_config_prompt(new_prompt);
                        }
                    }
                    if app.should_quit {
                        return Ok(());
                    }
                    continue;
                }

                if key.code == KeyCode::Char('q') {
                    return Ok(());
                }

                if key.code == KeyCode::Char('S') {
                    app.stop_running_implementation();
                    continue;
                }

                match &app.screen {
                    Screen::ChangeList { .. } => {
                        app.handle_change_list_input(key.code);
                    }
                    Screen::ArtifactMenu { .. } => {
                        app.handle_artifact_menu_input(key.code);
                    }
                    Screen::ArtifactView { .. } => {
                        app.handle_artifact_view_input(key.code);
                    }
                    Screen::Config { .. } => {
                        // Handled above
                    }
                }

                if app.should_quit {
                    return Ok(());
                }
            }
        }
    }
}
