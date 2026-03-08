use crossterm::event::KeyCode;
use std::path::PathBuf;

use crate::config::TuiConfig;
use crate::data::{self, ChangeEntry, ChangeStatusOutput};
use crate::runner::{self, stop_implementation, ImplState};
#[cfg(test)]
use crate::data::ArtifactStatus;

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeTab {
    Active,
    Archived,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigField {
    Command,
    Prompt,
}

#[derive(Debug, Clone)]
pub enum Screen {
    ChangeList {
        changes: Vec<ChangeEntry>,
        selected: usize,
        error: Option<String>,
        tab: ChangeTab,
    },
    ArtifactMenu {
        change_name: String,
        change_dir: PathBuf,
        items: Vec<ArtifactMenuItem>,
        selected: usize,
        is_archived: bool,
    },
    ArtifactView {
        title: String,
        content: String,
        scroll: usize,
        is_plain_text: bool,
    },
    Config {
        command: String,
        prompt: String,
        cursor_position: usize,
        focused_field: ConfigField,
        editing: bool,
    },
}

#[derive(Debug, Clone)]
pub struct ArtifactMenuItem {
    pub label: String,
    pub available: bool,
    pub file_path: Option<PathBuf>,
    pub is_spec_header: bool,
}

pub struct App {
    pub screen: Screen,
    pub screen_stack: Vec<Screen>,
    pub should_quit: bool,
    pub implementation: Option<ImplState>,
    pub config: TuiConfig,
}

impl App {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let screen = match data::list_changes() {
            Ok(list) => Screen::ChangeList {
                changes: list.changes,
                selected: 0,
                error: None,
                tab: ChangeTab::Active,
            },
            Err(e) => Screen::ChangeList {
                changes: Vec::new(),
                selected: 0,
                error: Some(e),
                tab: ChangeTab::Active,
            },
        };

        let config = TuiConfig::load()?;

        Ok(App {
            screen,
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config,
        })
    }

    pub fn poll_implementation(&mut self) {
        let should_clear = if let Some(ref mut state) = self.implementation {
            let mut clear = false;
            while let Ok(update) = state.receiver.try_recv() {
                match update {
                    runner::ImplUpdate::Progress { completed, total } => {
                        state.completed = completed;
                        state.total = total;
                    }
                    runner::ImplUpdate::Finished => {
                        clear = true;
                        break;
                    }
                }
            }
            clear
        } else {
            false
        };
        if should_clear {
            self.implementation = None;
        }
    }

    pub fn stop_running_implementation(&mut self) {
        if let Some(ref state) = self.implementation {
            stop_implementation(state);
            self.implementation = None;
        }
    }

    pub fn handle_change_list_input(&mut self, key: KeyCode) {
        let Screen::ChangeList {
            changes,
            selected,
            tab,
            ..
        } = &mut self.screen
        else {
            return;
        };

        match key {
            KeyCode::Down | KeyCode::Char('j') => {
                if !changes.is_empty() && *selected < changes.len() - 1 {
                    *selected += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if *selected > 0 {
                    *selected -= 1;
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if *tab == ChangeTab::Active {
                    *tab = ChangeTab::Archived;
                    *selected = 0;
                    match data::list_archived_changes() {
                        Ok(archived) => {
                            *changes = archived;
                        }
                        Err(_) => {
                            *changes = Vec::new();
                        }
                    }
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if *tab == ChangeTab::Archived {
                    *tab = ChangeTab::Active;
                    *selected = 0;
                    match data::list_changes() {
                        Ok(list) => {
                            *changes = list.changes;
                        }
                        Err(_) => {
                            *changes = Vec::new();
                        }
                    }
                }
            }
            KeyCode::Enter => {
                if changes.is_empty() {
                    return;
                }
                let change = &changes[*selected];
                let change_name = change.name.clone();
                let is_archived = *tab == ChangeTab::Archived;
                self.enter_artifact_menu(&change_name, is_archived);
            }
            KeyCode::Char('C') => {
                self.push_config_screen();
            }
            _ => {}
        }
    }

    fn enter_artifact_menu(&mut self, change_name: &str, is_archived: bool) {
        let change_dir = self.find_change_dir(change_name, is_archived);

        let status = if is_archived {
            data::get_archived_change_status(&change_dir)
        } else {
            match data::get_change_status(change_name) {
                Ok(s) => s,
                Err(_) => return,
            }
        };

        let items = build_artifact_menu_items(&status, &change_dir);

        let old_screen = std::mem::replace(
            &mut self.screen,
            Screen::ArtifactMenu {
                change_name: change_name.to_string(),
                change_dir,
                items,
                selected: 0,
                is_archived,
            },
        );
        self.screen_stack.push(old_screen);
    }

    fn find_change_dir(&self, change_name: &str, is_archived: bool) -> PathBuf {
        let cwd = std::env::current_dir().unwrap_or_default();
        if is_archived {
            cwd.join("openspec")
                .join("changes")
                .join("archive")
                .join(change_name)
        } else {
            cwd.join("openspec").join("changes").join(change_name)
        }
    }

    pub fn handle_artifact_menu_input(&mut self, key: KeyCode) {
        let Screen::ArtifactMenu {
            change_name,
            items,
            selected,
            change_dir,
            is_archived,
        } = &mut self.screen
        else {
            return;
        };

        match key {
            KeyCode::Down | KeyCode::Char('j') => {
                if !items.is_empty() && *selected < items.len() - 1 {
                    *selected += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if *selected > 0 {
                    *selected -= 1;
                }
            }
            KeyCode::Enter => {
                if items.is_empty() {
                    return;
                }
                let item = &items[*selected];
                if !item.available || item.is_spec_header {
                    return;
                }
                if let Some(path) = &item.file_path {
                    let title = item.label.clone();
                    let content = data::read_artifact_content(path)
                        .unwrap_or_else(|e| format!("Error reading file: {e}"));
                    let is_plain_text = path.extension().is_some_and(|ext| ext == "log");
                    let change_dir = change_dir.clone();
                    let old_screen = std::mem::replace(
                        &mut self.screen,
                        Screen::ArtifactView {
                            title,
                            content,
                            scroll: 0,
                            is_plain_text,
                        },
                    );
                    let _ = change_dir;
                    self.screen_stack.push(old_screen);
                }
            }
            KeyCode::Char('L') => {
                let log_path = change_dir.join("implementation.log");
                if log_path.exists() {
                    let content = data::read_artifact_content(&log_path)
                        .unwrap_or_else(|e| format!("Error reading file: {e}"));
                    let old_screen = std::mem::replace(
                        &mut self.screen,
                        Screen::ArtifactView {
                            title: "Implementation Log".to_string(),
                            content,
                            scroll: 0,
                            is_plain_text: true,
                        },
                    );
                    self.screen_stack.push(old_screen);
                }
            }
            KeyCode::Char('R') => {
                if !*is_archived && self.implementation.is_none() {
                    let name = change_name.clone();
                    let log_path = change_dir.clone().join("implementation.log");
                    self.implementation = Some(runner::start_implementation(&name, &self.config));
                    let content = data::read_artifact_content(&log_path)
                        .unwrap_or_default();
                    let old_screen = std::mem::replace(
                        &mut self.screen,
                        Screen::ArtifactView {
                            title: "Implementation Log".to_string(),
                            content,
                            scroll: 0,
                            is_plain_text: true,
                        },
                    );
                    self.screen_stack.push(old_screen);
                }
            }
            KeyCode::Char('C') => {
                self.push_config_screen();
            }
            KeyCode::Esc => {
                if let Some(prev) = self.screen_stack.pop() {
                    self.screen = prev;
                }
            }
            _ => {}
        }
    }

    pub fn push_config_screen(&mut self) {
        let old_screen = std::mem::replace(
            &mut self.screen,
            Screen::Config {
                command: self.config.command.clone(),
                prompt: self.config.prompt.clone(),
                cursor_position: self.config.command.len(),
                focused_field: ConfigField::Command,
                editing: false,
            },
        );
        self.screen_stack.push(old_screen);
    }

    /// Handle input on the Config screen. Returns `true` if the caller
    /// should open `$EDITOR` for the prompt field (Enter on Prompt).
    pub fn handle_config_input(&mut self, key: KeyCode) -> bool {
        let Screen::Config {
            command,
            prompt,
            cursor_position,
            focused_field,
            editing,
        } = &mut self.screen
        else {
            return false;
        };

        if *editing {
            // Edit mode (Command field only)
            match key {
                KeyCode::Esc | KeyCode::Enter => {
                    *editing = false;
                }
                KeyCode::Char(c) => {
                    command.insert(*cursor_position, c);
                    *cursor_position += 1;
                }
                KeyCode::Backspace => {
                    if *cursor_position > 0 {
                        *cursor_position -= 1;
                        command.remove(*cursor_position);
                    }
                }
                KeyCode::Delete => {
                    if *cursor_position < command.len() {
                        command.remove(*cursor_position);
                    }
                }
                KeyCode::Left => {
                    if *cursor_position > 0 {
                        *cursor_position -= 1;
                    }
                }
                KeyCode::Right => {
                    if *cursor_position < command.len() {
                        *cursor_position += 1;
                    }
                }
                KeyCode::Home => {
                    *cursor_position = 0;
                }
                KeyCode::End => {
                    *cursor_position = command.len();
                }
                _ => {}
            }
        } else {
            // Navigation mode
            match key {
                KeyCode::Tab | KeyCode::BackTab => {
                    *focused_field = match focused_field {
                        ConfigField::Command => ConfigField::Prompt,
                        ConfigField::Prompt => ConfigField::Command,
                    };
                    if *focused_field == ConfigField::Command {
                        *cursor_position = command.len();
                    }
                }
                KeyCode::Esc => {
                    // Discard changes and return to previous screen
                    if let Some(prev) = self.screen_stack.pop() {
                        self.screen = prev;
                    }
                }
                KeyCode::Enter => {
                    if *focused_field == ConfigField::Command {
                        *cursor_position = command.len();
                        *editing = true;
                    } else {
                        // Prompt field: signal caller to open $EDITOR
                        return true;
                    }
                }
                KeyCode::Char('S') => {
                    // Save config and return
                    let new_config = TuiConfig {
                        command: command.clone(),
                        prompt: prompt.clone(),
                    };
                    let _ = new_config.save();
                    self.config = new_config;
                    if let Some(prev) = self.screen_stack.pop() {
                        self.screen = prev;
                    }
                }
                KeyCode::Char('D') => {
                    // Reset to defaults
                    let defaults = TuiConfig::default();
                    *command = defaults.command;
                    *prompt = defaults.prompt;
                    *cursor_position = command.len();
                    *focused_field = ConfigField::Command;
                }
                _ => {} // Character keys ignored in navigation mode
            }
        }
        false
    }

    /// Update the prompt text on the Config screen (called after $EDITOR exits).
    pub fn set_config_prompt(&mut self, new_prompt: String) {
        if let Screen::Config { prompt, .. } = &mut self.screen {
            *prompt = new_prompt;
        }
    }

    pub fn handle_artifact_view_input(&mut self, key: KeyCode) {
        let Screen::ArtifactView {
            scroll, content, ..
        } = &mut self.screen
        else {
            return;
        };

        let line_count = content.lines().count();

        match key {
            KeyCode::Down | KeyCode::Char('j') => {
                if line_count > 0 && *scroll < line_count.saturating_sub(1) {
                    *scroll += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if *scroll > 0 {
                    *scroll -= 1;
                }
            }
            KeyCode::Char('C') => {
                self.push_config_screen();
            }
            KeyCode::Esc => {
                if let Some(prev) = self.screen_stack.pop() {
                    self.screen = prev;
                }
            }
            _ => {}
        }
    }
}

pub fn build_artifact_menu_items(
    status: &ChangeStatusOutput,
    change_dir: &PathBuf,
) -> Vec<ArtifactMenuItem> {
    let mut items = Vec::new();

    let artifact_defs = [
        ("proposal", "Proposal", "proposal.md"),
        ("design", "Design", "design.md"),
        ("tasks", "Tasks", "tasks.md"),
    ];

    for (id, label, filename) in &artifact_defs {
        let artifact = status.artifacts.iter().find(|a| a.id == *id);
        let available = artifact.is_some_and(|a| a.status == "done");
        let file_path = if available {
            Some(change_dir.join(filename))
        } else {
            None
        };

        items.push(ArtifactMenuItem {
            label: label.to_string(),
            available,
            file_path,
            is_spec_header: false,
        });
    }

    // Specs header + sub-items
    let specs_artifact = status.artifacts.iter().find(|a| a.id == "specs");
    let specs_available = specs_artifact.is_some_and(|a| a.status == "done");
    let spec_items = if specs_available {
        data::discover_specs(change_dir)
    } else {
        Vec::new()
    };

    items.push(ArtifactMenuItem {
        label: "Specs".to_string(),
        available: specs_available,
        file_path: None,
        is_spec_header: true,
    });

    for spec in &spec_items {
        items.push(ArtifactMenuItem {
            label: format!("  {}", spec.name),
            available: true,
            file_path: Some(spec.path.clone()),
            is_spec_header: false,
        });
    }

    // Add implementation.log entry if the file exists
    let log_path = change_dir.join("implementation.log");
    if log_path.exists() {
        items.push(ArtifactMenuItem {
            label: "Implementation Log".to_string(),
            available: true,
            file_path: Some(log_path),
            is_spec_header: false,
        });
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_status(artifacts: Vec<(&str, &str)>) -> ChangeStatusOutput {
        ChangeStatusOutput {
            change_name: "test-change".to_string(),
            schema_name: "spec-driven".to_string(),
            artifacts: artifacts
                .into_iter()
                .map(|(id, status)| ArtifactStatus {
                    id: id.to_string(),
                    output_path: String::new(),
                    status: status.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn test_screen_transition_change_list_to_artifact_menu() {
        let mut app = App {
            screen: Screen::ChangeList {
                changes: vec![ChangeEntry {
                    name: "test".to_string(),
                    completed_tasks: 0,
                    total_tasks: 5,
                    status: "in-progress".to_string(),
                }],
                selected: 0,
                error: None,
                tab: ChangeTab::Active,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        // Pressing Esc on ChangeList shouldn't crash (no parent screen)
        app.handle_change_list_input(KeyCode::Esc);
        assert!(matches!(app.screen, Screen::ChangeList { .. }));
    }

    #[test]
    fn test_screen_transition_esc_from_artifact_menu() {
        let original_screen = Screen::ChangeList {
            changes: vec![],
            selected: 0,
            error: None,
            tab: ChangeTab::Active,
        };

        let mut app = App {
            screen: Screen::ArtifactMenu {
                change_name: "test".to_string(),
                change_dir: PathBuf::from("/tmp"),
                items: vec![],
                selected: 0,
                is_archived: false,
            },
            screen_stack: vec![original_screen],
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        app.handle_artifact_menu_input(KeyCode::Esc);
        assert!(matches!(app.screen, Screen::ChangeList { .. }));
        assert!(app.screen_stack.is_empty());
    }

    #[test]
    fn test_screen_transition_esc_from_artifact_view() {
        let menu_screen = Screen::ArtifactMenu {
            change_name: "test".to_string(),
            change_dir: PathBuf::from("/tmp"),
            items: vec![],
            selected: 0,
            is_archived: false,
        };

        let mut app = App {
            screen: Screen::ArtifactView {
                title: "Proposal".to_string(),
                content: "hello\nworld".to_string(),
                scroll: 0,
                is_plain_text: false,
            },
            screen_stack: vec![menu_screen],
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        app.handle_artifact_view_input(KeyCode::Esc);
        assert!(matches!(app.screen, Screen::ArtifactMenu { .. }));
    }

    #[test]
    fn test_change_list_navigation() {
        let mut app = App {
            screen: Screen::ChangeList {
                changes: vec![
                    ChangeEntry {
                        name: "a".to_string(),
                        completed_tasks: 0,
                        total_tasks: 1,
                        status: "in-progress".to_string(),
                    },
                    ChangeEntry {
                        name: "b".to_string(),
                        completed_tasks: 0,
                        total_tasks: 1,
                        status: "in-progress".to_string(),
                    },
                    ChangeEntry {
                        name: "c".to_string(),
                        completed_tasks: 0,
                        total_tasks: 1,
                        status: "in-progress".to_string(),
                    },
                ],
                selected: 0,
                error: None,
                tab: ChangeTab::Active,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        // Move down
        app.handle_change_list_input(KeyCode::Char('j'));
        if let Screen::ChangeList { selected, .. } = &app.screen {
            assert_eq!(*selected, 1);
        }

        // Move down again
        app.handle_change_list_input(KeyCode::Down);
        if let Screen::ChangeList { selected, .. } = &app.screen {
            assert_eq!(*selected, 2);
        }

        // At bottom, stays
        app.handle_change_list_input(KeyCode::Down);
        if let Screen::ChangeList { selected, .. } = &app.screen {
            assert_eq!(*selected, 2);
        }

        // Move up
        app.handle_change_list_input(KeyCode::Char('k'));
        if let Screen::ChangeList { selected, .. } = &app.screen {
            assert_eq!(*selected, 1);
        }

        app.handle_change_list_input(KeyCode::Up);
        if let Screen::ChangeList { selected, .. } = &app.screen {
            assert_eq!(*selected, 0);
        }

        // At top, stays
        app.handle_change_list_input(KeyCode::Up);
        if let Screen::ChangeList { selected, .. } = &app.screen {
            assert_eq!(*selected, 0);
        }
    }

    #[test]
    fn test_artifact_view_scrolling() {
        let mut app = App {
            screen: Screen::ArtifactView {
                title: "Test".to_string(),
                content: "line1\nline2\nline3\nline4\nline5".to_string(),
                scroll: 0,
                is_plain_text: false,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        // Scroll down
        app.handle_artifact_view_input(KeyCode::Char('j'));
        if let Screen::ArtifactView { scroll, .. } = &app.screen {
            assert_eq!(*scroll, 1);
        }

        app.handle_artifact_view_input(KeyCode::Down);
        if let Screen::ArtifactView { scroll, .. } = &app.screen {
            assert_eq!(*scroll, 2);
        }

        // Scroll up
        app.handle_artifact_view_input(KeyCode::Char('k'));
        if let Screen::ArtifactView { scroll, .. } = &app.screen {
            assert_eq!(*scroll, 1);
        }

        app.handle_artifact_view_input(KeyCode::Up);
        if let Screen::ArtifactView { scroll, .. } = &app.screen {
            assert_eq!(*scroll, 0);
        }

        // At top, stays
        app.handle_artifact_view_input(KeyCode::Up);
        if let Screen::ArtifactView { scroll, .. } = &app.screen {
            assert_eq!(*scroll, 0);
        }
    }

    #[test]
    fn test_artifact_menu_navigation() {
        let items = vec![
            ArtifactMenuItem {
                label: "Proposal".to_string(),
                available: true,
                file_path: Some(PathBuf::from("/tmp/proposal.md")),
                is_spec_header: false,
            },
            ArtifactMenuItem {
                label: "Design".to_string(),
                available: false,
                file_path: None,
                is_spec_header: false,
            },
            ArtifactMenuItem {
                label: "Tasks".to_string(),
                available: true,
                file_path: Some(PathBuf::from("/tmp/tasks.md")),
                is_spec_header: false,
            },
        ];

        let mut app = App {
            screen: Screen::ArtifactMenu {
                change_name: "test".to_string(),
                change_dir: PathBuf::from("/tmp"),
                items,
                selected: 0,
                is_archived: false,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        app.handle_artifact_menu_input(KeyCode::Down);
        if let Screen::ArtifactMenu { selected, .. } = &app.screen {
            assert_eq!(*selected, 1);
        }

        // Enter on unavailable item -> no transition
        app.handle_artifact_menu_input(KeyCode::Enter);
        assert!(matches!(app.screen, Screen::ArtifactMenu { .. }));
    }

    #[test]
    fn test_build_artifact_menu_items_all_done() {
        let status = make_status(vec![
            ("proposal", "done"),
            ("design", "done"),
            ("tasks", "done"),
            ("specs", "done"),
        ]);
        let change_dir = PathBuf::from("/tmp/nonexistent");
        let items = build_artifact_menu_items(&status, &change_dir);

        assert_eq!(items[0].label, "Proposal");
        assert!(items[0].available);
        assert_eq!(items[1].label, "Design");
        assert!(items[1].available);
        assert_eq!(items[2].label, "Tasks");
        assert!(items[2].available);
        assert_eq!(items[3].label, "Specs");
        assert!(items[3].available);
        assert!(items[3].is_spec_header);
    }

    #[test]
    fn test_build_artifact_menu_items_some_pending() {
        let status = make_status(vec![
            ("proposal", "done"),
            ("design", "pending"),
            ("tasks", "done"),
            ("specs", "pending"),
        ]);
        let change_dir = PathBuf::from("/tmp/nonexistent");
        let items = build_artifact_menu_items(&status, &change_dir);

        assert!(items[0].available); // proposal done
        assert!(!items[1].available); // design pending
        assert!(items[2].available); // tasks done
        assert!(!items[3].available); // specs pending
    }

    #[test]
    fn test_enter_on_unavailable_artifact_is_noop() {
        let mut app = App {
            screen: Screen::ArtifactMenu {
                change_name: "test".to_string(),
                change_dir: PathBuf::from("/tmp"),
                items: vec![ArtifactMenuItem {
                    label: "Design".to_string(),
                    available: false,
                    file_path: None,
                    is_spec_header: false,
                }],
                selected: 0,
                is_archived: false,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        app.handle_artifact_menu_input(KeyCode::Enter);
        // Should still be on ArtifactMenu, not transitioned
        assert!(matches!(app.screen, Screen::ArtifactMenu { .. }));
    }

    #[test]
    fn test_enter_on_spec_header_is_noop() {
        let mut app = App {
            screen: Screen::ArtifactMenu {
                change_name: "test".to_string(),
                change_dir: PathBuf::from("/tmp"),
                items: vec![ArtifactMenuItem {
                    label: "Specs".to_string(),
                    available: true,
                    file_path: None,
                    is_spec_header: true,
                }],
                selected: 0,
                is_archived: false,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        app.handle_artifact_menu_input(KeyCode::Enter);
        assert!(matches!(app.screen, Screen::ArtifactMenu { .. }));
    }

    #[test]
    fn test_r_key_starts_implementation() {
        let mut app = App {
            screen: Screen::ArtifactMenu {
                change_name: "test-change".to_string(),
                change_dir: PathBuf::from("/tmp"),
                items: vec![],
                selected: 0,
                is_archived: false,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        assert!(app.implementation.is_none());
        app.handle_artifact_menu_input(KeyCode::Char('R'));
        assert!(app.implementation.is_some());
        assert_eq!(
            app.implementation.as_ref().unwrap().change_name,
            "test-change"
        );
    }

    #[test]
    fn test_r_key_ignored_when_implementation_running() {
        use std::sync::atomic::AtomicBool;
        use std::sync::{mpsc, Arc, Mutex};

        let (_tx, rx) = mpsc::channel();
        let existing_impl = crate::runner::ImplState {
            change_name: "existing-change".to_string(),
            completed: 1,
            total: 5,
            log_path: PathBuf::from("/tmp/existing.log"),
            receiver: rx,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            child_handle: Arc::new(Mutex::new(None)),
        };

        let mut app = App {
            screen: Screen::ArtifactMenu {
                change_name: "new-change".to_string(),
                change_dir: PathBuf::from("/tmp"),
                items: vec![],
                selected: 0,
                is_archived: false,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: Some(existing_impl),
            config: TuiConfig::default(),
        };

        app.handle_artifact_menu_input(KeyCode::Char('R'));
        // Should still be the existing implementation, not replaced
        assert_eq!(
            app.implementation.as_ref().unwrap().change_name,
            "existing-change"
        );
    }

    #[test]
    fn test_s_key_stops_implementation() {
        use std::sync::atomic::AtomicBool;
        use std::sync::{mpsc, Arc, Mutex};

        let (_tx, rx) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let cancel_flag_clone = cancel_flag.clone();
        let existing_impl = crate::runner::ImplState {
            change_name: "test-change".to_string(),
            completed: 1,
            total: 5,
            log_path: PathBuf::from("/tmp/test.log"),
            receiver: rx,
            cancel_flag,
            child_handle: Arc::new(Mutex::new(None)),
        };

        let mut app = App {
            screen: Screen::ChangeList {
                changes: vec![],
                selected: 0,
                error: None,
                tab: ChangeTab::Active,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: Some(existing_impl),
            config: TuiConfig::default(),
        };

        assert!(app.implementation.is_some());
        app.stop_running_implementation();
        assert!(app.implementation.is_none());
        assert!(cancel_flag_clone.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn test_s_key_noop_when_no_implementation() {
        let mut app = App {
            screen: Screen::ChangeList {
                changes: vec![],
                selected: 0,
                error: None,
                tab: ChangeTab::Active,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        // Should not panic when no implementation is running
        app.stop_running_implementation();
        assert!(app.implementation.is_none());
    }

    #[test]
    fn test_s_key_works_from_artifact_view() {
        use std::sync::atomic::AtomicBool;
        use std::sync::{mpsc, Arc, Mutex};

        let (_tx, rx) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let cancel_flag_clone = cancel_flag.clone();
        let existing_impl = crate::runner::ImplState {
            change_name: "test-change".to_string(),
            completed: 2,
            total: 5,
            log_path: PathBuf::from("/tmp/test.log"),
            receiver: rx,
            cancel_flag,
            child_handle: Arc::new(Mutex::new(None)),
        };

        let mut app = App {
            screen: Screen::ArtifactView {
                title: "Test".to_string(),
                content: "content".to_string(),
                scroll: 0,
                is_plain_text: false,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: Some(existing_impl),
            config: TuiConfig::default(),
        };

        app.stop_running_implementation();
        assert!(app.implementation.is_none());
        assert!(cancel_flag_clone.load(std::sync::atomic::Ordering::Relaxed));
        // Screen should remain unchanged
        assert!(matches!(app.screen, Screen::ArtifactView { .. }));
    }

    #[test]
    fn test_s_key_works_from_artifact_menu() {
        use std::sync::atomic::AtomicBool;
        use std::sync::{mpsc, Arc, Mutex};

        let (_tx, rx) = mpsc::channel();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let cancel_flag_clone = cancel_flag.clone();
        let existing_impl = crate::runner::ImplState {
            change_name: "test-change".to_string(),
            completed: 3,
            total: 5,
            log_path: PathBuf::from("/tmp/test.log"),
            receiver: rx,
            cancel_flag,
            child_handle: Arc::new(Mutex::new(None)),
        };

        let mut app = App {
            screen: Screen::ArtifactMenu {
                change_name: "test-change".to_string(),
                change_dir: PathBuf::from("/tmp"),
                items: vec![],
                selected: 0,
                is_archived: false,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: Some(existing_impl),
            config: TuiConfig::default(),
        };

        app.stop_running_implementation();
        assert!(app.implementation.is_none());
        assert!(cancel_flag_clone.load(std::sync::atomic::Ordering::Relaxed));
        // Screen should remain unchanged
        assert!(matches!(app.screen, Screen::ArtifactMenu { .. }));
    }

    #[test]
    fn test_poll_implementation_updates_progress() {
        use std::sync::atomic::AtomicBool;
        use std::sync::{mpsc, Arc, Mutex};

        let (tx, rx) = mpsc::channel();
        let impl_state = crate::runner::ImplState {
            change_name: "test-change".to_string(),
            completed: 0,
            total: 5,
            log_path: PathBuf::from("/tmp/test.log"),
            receiver: rx,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            child_handle: Arc::new(Mutex::new(None)),
        };

        let mut app = App {
            screen: Screen::ChangeList {
                changes: vec![],
                selected: 0,
                error: None,
                tab: ChangeTab::Active,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: Some(impl_state),
            config: TuiConfig::default(),
        };

        // Send progress updates
        tx.send(crate::runner::ImplUpdate::Progress {
            completed: 2,
            total: 5,
        })
        .unwrap();
        tx.send(crate::runner::ImplUpdate::Progress {
            completed: 3,
            total: 5,
        })
        .unwrap();

        app.poll_implementation();

        // Should have consumed all messages and applied the latest progress
        let state = app.implementation.as_ref().unwrap();
        assert_eq!(state.completed, 3);
        assert_eq!(state.total, 5);
    }

    #[test]
    fn test_poll_implementation_clears_on_finished() {
        use std::sync::atomic::AtomicBool;
        use std::sync::{mpsc, Arc, Mutex};

        let (tx, rx) = mpsc::channel();
        let impl_state = crate::runner::ImplState {
            change_name: "test-change".to_string(),
            completed: 3,
            total: 5,
            log_path: PathBuf::from("/tmp/test.log"),
            receiver: rx,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            child_handle: Arc::new(Mutex::new(None)),
        };

        let mut app = App {
            screen: Screen::ChangeList {
                changes: vec![],
                selected: 0,
                error: None,
                tab: ChangeTab::Active,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: Some(impl_state),
            config: TuiConfig::default(),
        };

        tx.send(crate::runner::ImplUpdate::Finished).unwrap();

        app.poll_implementation();

        assert!(app.implementation.is_none());
    }

    #[test]
    fn test_poll_implementation_progress_then_finished() {
        use std::sync::atomic::AtomicBool;
        use std::sync::{mpsc, Arc, Mutex};

        let (tx, rx) = mpsc::channel();
        let impl_state = crate::runner::ImplState {
            change_name: "test-change".to_string(),
            completed: 0,
            total: 5,
            log_path: PathBuf::from("/tmp/test.log"),
            receiver: rx,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            child_handle: Arc::new(Mutex::new(None)),
        };

        let mut app = App {
            screen: Screen::ChangeList {
                changes: vec![],
                selected: 0,
                error: None,
                tab: ChangeTab::Active,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: Some(impl_state),
            config: TuiConfig::default(),
        };

        // Send progress then finished
        tx.send(crate::runner::ImplUpdate::Progress {
            completed: 5,
            total: 5,
        })
        .unwrap();
        tx.send(crate::runner::ImplUpdate::Finished).unwrap();

        app.poll_implementation();

        // Finished should clear the implementation
        assert!(app.implementation.is_none());
    }

    #[test]
    fn test_poll_implementation_noop_when_none() {
        let mut app = App {
            screen: Screen::ChangeList {
                changes: vec![],
                selected: 0,
                error: None,
                tab: ChangeTab::Active,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        // Should not panic
        app.poll_implementation();
        assert!(app.implementation.is_none());
    }

    #[test]
    fn test_poll_implementation_no_messages() {
        use std::sync::atomic::AtomicBool;
        use std::sync::{mpsc, Arc, Mutex};

        let (_tx, rx) = mpsc::channel();
        let impl_state = crate::runner::ImplState {
            change_name: "test-change".to_string(),
            completed: 2,
            total: 5,
            log_path: PathBuf::from("/tmp/test.log"),
            receiver: rx,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            child_handle: Arc::new(Mutex::new(None)),
        };

        let mut app = App {
            screen: Screen::ChangeList {
                changes: vec![],
                selected: 0,
                error: None,
                tab: ChangeTab::Active,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: Some(impl_state),
            config: TuiConfig::default(),
        };

        app.poll_implementation();

        // Should still be running with unchanged progress
        let state = app.implementation.as_ref().unwrap();
        assert_eq!(state.completed, 2);
        assert_eq!(state.total, 5);
    }

    #[test]
    fn test_build_artifact_menu_items_with_implementation_log() {
        let dir = std::env::temp_dir().join("openspec-tui-test-menu-with-log");
        std::fs::create_dir_all(&dir).unwrap();
        // Create the implementation.log file
        std::fs::write(dir.join("implementation.log"), "log content").unwrap();

        let status = make_status(vec![
            ("proposal", "done"),
            ("design", "done"),
            ("tasks", "done"),
            ("specs", "pending"),
        ]);
        let items = build_artifact_menu_items(&status, &dir);

        // Should have: Proposal, Design, Tasks, Specs header, Implementation Log
        let last = items.last().unwrap();
        assert_eq!(last.label, "Implementation Log");
        assert!(last.available);
        assert_eq!(last.file_path, Some(dir.join("implementation.log")));
        assert!(!last.is_spec_header);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_build_artifact_menu_items_without_implementation_log() {
        let dir = std::env::temp_dir().join("openspec-tui-test-menu-no-log");
        std::fs::create_dir_all(&dir).unwrap();
        // Do NOT create implementation.log

        let status = make_status(vec![
            ("proposal", "done"),
            ("design", "done"),
            ("tasks", "done"),
            ("specs", "pending"),
        ]);
        let items = build_artifact_menu_items(&status, &dir);

        // No item should have the "Implementation Log" label
        assert!(
            !items.iter().any(|i| i.label == "Implementation Log"),
            "Implementation Log should not appear when file does not exist"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_change_list_empty_navigation() {
        let mut app = App {
            screen: Screen::ChangeList {
                changes: vec![],
                selected: 0,
                error: None,
                tab: ChangeTab::Active,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        // Navigation on empty list shouldn't panic
        app.handle_change_list_input(KeyCode::Down);
        app.handle_change_list_input(KeyCode::Up);
        app.handle_change_list_input(KeyCode::Enter);
        assert!(matches!(app.screen, Screen::ChangeList { .. }));
    }

    #[test]
    fn test_tab_switch_active_to_archived() {
        let mut app = App {
            screen: Screen::ChangeList {
                changes: vec![ChangeEntry {
                    name: "active-change".to_string(),
                    completed_tasks: 0,
                    total_tasks: 1,
                    status: "in-progress".to_string(),
                }],
                selected: 0,
                error: None,
                tab: ChangeTab::Active,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        app.handle_change_list_input(KeyCode::Right);
        if let Screen::ChangeList { tab, selected, .. } = &app.screen {
            assert_eq!(*tab, ChangeTab::Archived);
            assert_eq!(*selected, 0);
        } else {
            panic!("Expected ChangeList screen");
        }
    }

    #[test]
    fn test_tab_switch_archived_to_active() {
        let mut app = App {
            screen: Screen::ChangeList {
                changes: vec![],
                selected: 0,
                error: None,
                tab: ChangeTab::Archived,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        app.handle_change_list_input(KeyCode::Left);
        if let Screen::ChangeList { tab, .. } = &app.screen {
            assert_eq!(*tab, ChangeTab::Active);
        } else {
            panic!("Expected ChangeList screen");
        }
    }

    #[test]
    fn test_tab_switch_already_on_active_left_noop() {
        let mut app = App {
            screen: Screen::ChangeList {
                changes: vec![ChangeEntry {
                    name: "test".to_string(),
                    completed_tasks: 0,
                    total_tasks: 1,
                    status: "in-progress".to_string(),
                }],
                selected: 0,
                error: None,
                tab: ChangeTab::Active,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        app.handle_change_list_input(KeyCode::Left);
        if let Screen::ChangeList { tab, changes, .. } = &app.screen {
            assert_eq!(*tab, ChangeTab::Active);
            // Changes should not be reloaded (still has the original entry)
            assert_eq!(changes.len(), 1);
            assert_eq!(changes[0].name, "test");
        } else {
            panic!("Expected ChangeList screen");
        }
    }

    #[test]
    fn test_tab_switch_already_on_archived_right_noop() {
        let mut app = App {
            screen: Screen::ChangeList {
                changes: vec![],
                selected: 0,
                error: None,
                tab: ChangeTab::Archived,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        app.handle_change_list_input(KeyCode::Right);
        if let Screen::ChangeList { tab, .. } = &app.screen {
            assert_eq!(*tab, ChangeTab::Archived);
        } else {
            panic!("Expected ChangeList screen");
        }
    }

    #[test]
    fn test_tab_switch_with_h_l_keys() {
        let mut app = App {
            screen: Screen::ChangeList {
                changes: vec![],
                selected: 0,
                error: None,
                tab: ChangeTab::Active,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        // Switch to archived with 'l'
        app.handle_change_list_input(KeyCode::Char('l'));
        if let Screen::ChangeList { tab, .. } = &app.screen {
            assert_eq!(*tab, ChangeTab::Archived);
        }

        // Switch back to active with 'h'
        app.handle_change_list_input(KeyCode::Char('h'));
        if let Screen::ChangeList { tab, .. } = &app.screen {
            assert_eq!(*tab, ChangeTab::Active);
        }
    }

    #[test]
    fn test_tab_switch_resets_selection() {
        let mut app = App {
            screen: Screen::ChangeList {
                changes: vec![
                    ChangeEntry {
                        name: "a".to_string(),
                        completed_tasks: 0,
                        total_tasks: 1,
                        status: "in-progress".to_string(),
                    },
                    ChangeEntry {
                        name: "b".to_string(),
                        completed_tasks: 0,
                        total_tasks: 1,
                        status: "in-progress".to_string(),
                    },
                ],
                selected: 1,
                error: None,
                tab: ChangeTab::Active,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        app.handle_change_list_input(KeyCode::Right);
        if let Screen::ChangeList { selected, .. } = &app.screen {
            assert_eq!(*selected, 0, "Selection should reset to 0 on tab switch");
        }
    }

    #[test]
    fn test_r_key_ignored_on_archived_change() {
        let mut app = App {
            screen: Screen::ArtifactMenu {
                change_name: "archived-change".to_string(),
                change_dir: PathBuf::from("/tmp"),
                items: vec![],
                selected: 0,
                is_archived: true,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        app.handle_artifact_menu_input(KeyCode::Char('R'));
        assert!(
            app.implementation.is_none(),
            "Implementation runner should not start for archived changes"
        );
    }

    #[test]
    fn test_r_key_works_on_active_change() {
        let mut app = App {
            screen: Screen::ArtifactMenu {
                change_name: "active-change".to_string(),
                change_dir: PathBuf::from("/tmp"),
                items: vec![],
                selected: 0,
                is_archived: false,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        app.handle_artifact_menu_input(KeyCode::Char('R'));
        assert!(
            app.implementation.is_some(),
            "Implementation runner should start for active changes"
        );
    }

    #[test]
    fn test_find_change_dir_active() {
        let app = App {
            screen: Screen::ChangeList {
                changes: vec![],
                selected: 0,
                error: None,
                tab: ChangeTab::Active,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        let dir = app.find_change_dir("my-change", false);
        assert!(dir.ends_with("openspec/changes/my-change"));
        assert!(!dir.to_string_lossy().contains("archive"));
    }

    #[test]
    fn test_find_change_dir_archived() {
        let app = App {
            screen: Screen::ChangeList {
                changes: vec![],
                selected: 0,
                error: None,
                tab: ChangeTab::Archived,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        let dir = app.find_change_dir("2026-03-06-my-change", true);
        assert!(dir.ends_with("openspec/changes/archive/2026-03-06-my-change"));
    }

    #[test]
    fn test_l_key_opens_log_when_exists() {
        let dir = std::env::temp_dir().join("openspec-tui-test-l-key");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("implementation.log"), "log line 1\nlog line 2").unwrap();

        let mut app = App {
            screen: Screen::ArtifactMenu {
                change_name: "test-change".to_string(),
                change_dir: dir.clone(),
                items: vec![],
                selected: 0,
                is_archived: false,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        app.handle_artifact_menu_input(KeyCode::Char('L'));

        if let Screen::ArtifactView {
            title,
            content,
            is_plain_text,
            ..
        } = &app.screen
        {
            assert_eq!(title, "Implementation Log");
            assert!(content.contains("log line 1"));
            assert!(*is_plain_text);
        } else {
            panic!("Expected ArtifactView screen after pressing L");
        }
        assert_eq!(app.screen_stack.len(), 1);
        assert!(matches!(app.screen_stack[0], Screen::ArtifactMenu { .. }));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_l_key_noop_when_log_missing() {
        let dir = std::env::temp_dir().join("openspec-tui-test-l-key-noop");
        std::fs::create_dir_all(&dir).unwrap();
        // No implementation.log created

        let mut app = App {
            screen: Screen::ArtifactMenu {
                change_name: "test-change".to_string(),
                change_dir: dir.clone(),
                items: vec![],
                selected: 0,
                is_archived: false,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        app.handle_artifact_menu_input(KeyCode::Char('L'));

        assert!(
            matches!(app.screen, Screen::ArtifactMenu { .. }),
            "Screen should remain ArtifactMenu when log does not exist"
        );
        assert!(app.screen_stack.is_empty());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_l_key_works_for_archived_changes() {
        let dir = std::env::temp_dir().join("openspec-tui-test-l-key-archived");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("implementation.log"), "archived log").unwrap();

        let mut app = App {
            screen: Screen::ArtifactMenu {
                change_name: "archived-change".to_string(),
                change_dir: dir.clone(),
                items: vec![],
                selected: 0,
                is_archived: true,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        app.handle_artifact_menu_input(KeyCode::Char('L'));

        if let Screen::ArtifactView {
            content,
            is_plain_text,
            ..
        } = &app.screen
        {
            assert!(content.contains("archived log"));
            assert!(*is_plain_text);
        } else {
            panic!("Expected ArtifactView screen after pressing L on archived change");
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_r_key_navigates_to_log_view() {
        let mut app = App {
            screen: Screen::ArtifactMenu {
                change_name: "active-change".to_string(),
                change_dir: PathBuf::from("/tmp"),
                items: vec![],
                selected: 0,
                is_archived: false,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        app.handle_artifact_menu_input(KeyCode::Char('R'));

        // Runner should have started
        assert!(app.implementation.is_some());
        // Should navigate to ArtifactView with plain text
        if let Screen::ArtifactView {
            title,
            is_plain_text,
            ..
        } = &app.screen
        {
            assert_eq!(title, "Implementation Log");
            assert!(*is_plain_text);
        } else {
            panic!("Expected ArtifactView after pressing R");
        }
        // Previous screen should be on the stack
        assert_eq!(app.screen_stack.len(), 1);
        assert!(matches!(app.screen_stack[0], Screen::ArtifactMenu { .. }));
    }

    #[test]
    fn test_r_key_esc_returns_to_artifact_menu() {
        let mut app = App {
            screen: Screen::ArtifactMenu {
                change_name: "active-change".to_string(),
                change_dir: PathBuf::from("/tmp"),
                items: vec![],
                selected: 0,
                is_archived: false,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };

        app.handle_artifact_menu_input(KeyCode::Char('R'));
        assert!(matches!(app.screen, Screen::ArtifactView { .. }));

        // Press Esc to go back
        app.handle_artifact_view_input(KeyCode::Esc);
        assert!(
            matches!(app.screen, Screen::ArtifactMenu { .. }),
            "Esc should return to ArtifactMenu"
        );
    }

    // --- Config screen tests ---

    fn make_config_app() -> App {
        App {
            screen: Screen::ChangeList {
                changes: vec![],
                selected: 0,
                error: None,
                tab: ChangeTab::Active,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig {
                command: "test-tool {prompt}".to_string(),
                prompt: "test prompt {name}".to_string(),
            },
        }
    }

    #[test]
    fn test_c_key_pushes_config_screen_from_change_list() {
        let mut app = make_config_app();
        app.handle_change_list_input(KeyCode::Char('C'));
        assert!(matches!(app.screen, Screen::Config { .. }));
        assert_eq!(app.screen_stack.len(), 1);
        assert!(matches!(app.screen_stack[0], Screen::ChangeList { .. }));
    }

    #[test]
    fn test_c_key_pushes_config_screen_from_artifact_menu() {
        let mut app = App {
            screen: Screen::ArtifactMenu {
                change_name: "test".to_string(),
                change_dir: PathBuf::from("/tmp"),
                items: vec![],
                selected: 0,
                is_archived: false,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };
        app.handle_artifact_menu_input(KeyCode::Char('C'));
        assert!(matches!(app.screen, Screen::Config { .. }));
    }

    #[test]
    fn test_c_key_pushes_config_screen_from_artifact_view() {
        let mut app = App {
            screen: Screen::ArtifactView {
                title: "Test".to_string(),
                content: "content".to_string(),
                scroll: 0,
                is_plain_text: false,
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
            config: TuiConfig::default(),
        };
        app.handle_artifact_view_input(KeyCode::Char('C'));
        assert!(matches!(app.screen, Screen::Config { .. }));
    }

    #[test]
    fn test_config_screen_has_cloned_config_values() {
        let mut app = make_config_app();
        app.push_config_screen();
        if let Screen::Config { command, prompt, .. } = &app.screen {
            assert_eq!(command, "test-tool {prompt}");
            assert_eq!(prompt, "test prompt {name}");
        } else {
            panic!("Expected Config screen");
        }
    }

    #[test]
    fn test_config_screen_cursor_starts_at_end() {
        let mut app = make_config_app();
        app.push_config_screen();
        if let Screen::Config { cursor_position, command, .. } = &app.screen {
            assert_eq!(*cursor_position, command.len());
        } else {
            panic!("Expected Config screen");
        }
    }

    #[test]
    fn test_config_screen_focused_on_command() {
        let mut app = make_config_app();
        app.push_config_screen();
        if let Screen::Config { focused_field, .. } = &app.screen {
            assert_eq!(*focused_field, ConfigField::Command);
        } else {
            panic!("Expected Config screen");
        }
    }

    #[test]
    fn test_config_esc_discards_changes() {
        let mut app = make_config_app();
        app.push_config_screen();

        // Type a character
        app.handle_config_input(KeyCode::Char('X'));

        // Press Esc to discard
        app.handle_config_input(KeyCode::Esc);
        assert!(matches!(app.screen, Screen::ChangeList { .. }));
        // Original config should be unchanged
        assert_eq!(app.config.command, "test-tool {prompt}");
    }

    #[test]
    fn test_config_tab_switches_focus() {
        let mut app = make_config_app();
        app.push_config_screen();

        // Start on Command
        if let Screen::Config { focused_field, .. } = &app.screen {
            assert_eq!(*focused_field, ConfigField::Command);
        }

        // Tab -> Prompt
        app.handle_config_input(KeyCode::Tab);
        if let Screen::Config { focused_field, .. } = &app.screen {
            assert_eq!(*focused_field, ConfigField::Prompt);
        }

        // Tab -> Command
        app.handle_config_input(KeyCode::Tab);
        if let Screen::Config { focused_field, .. } = &app.screen {
            assert_eq!(*focused_field, ConfigField::Command);
        }
    }

    #[test]
    fn test_config_typing_in_command_field() {
        let mut app = make_config_app();
        app.push_config_screen();

        // Enter edit mode, then move cursor to beginning
        app.handle_config_input(KeyCode::Enter);
        app.handle_config_input(KeyCode::Home);
        // Type characters
        app.handle_config_input(KeyCode::Char('A'));
        app.handle_config_input(KeyCode::Char('B'));

        if let Screen::Config { command, cursor_position, .. } = &app.screen {
            assert_eq!(command, "ABtest-tool {prompt}");
            assert_eq!(*cursor_position, 2);
        } else {
            panic!("Expected Config screen");
        }
    }

    #[test]
    fn test_config_backspace_in_command_field() {
        let mut app = make_config_app();
        app.push_config_screen();

        // Enter edit mode, then backspace deletes last char
        app.handle_config_input(KeyCode::Enter);
        app.handle_config_input(KeyCode::Backspace);
        if let Screen::Config { command, .. } = &app.screen {
            assert_eq!(command, "test-tool {prompt");
        }
    }

    #[test]
    fn test_config_delete_in_command_field() {
        let mut app = make_config_app();
        app.push_config_screen();

        // Enter edit mode, move to start, delete first char
        app.handle_config_input(KeyCode::Enter);
        app.handle_config_input(KeyCode::Home);
        app.handle_config_input(KeyCode::Delete);
        if let Screen::Config { command, .. } = &app.screen {
            assert_eq!(command, "est-tool {prompt}");
        }
    }

    #[test]
    fn test_config_cursor_movement() {
        let mut app = make_config_app();
        app.push_config_screen();

        let cmd_len = if let Screen::Config { command, .. } = &app.screen {
            command.len()
        } else {
            0
        };

        // Enter edit mode first
        app.handle_config_input(KeyCode::Enter);

        // Home -> cursor at 0
        app.handle_config_input(KeyCode::Home);
        if let Screen::Config { cursor_position, .. } = &app.screen {
            assert_eq!(*cursor_position, 0);
        }

        // Right -> cursor at 1
        app.handle_config_input(KeyCode::Right);
        if let Screen::Config { cursor_position, .. } = &app.screen {
            assert_eq!(*cursor_position, 1);
        }

        // End -> cursor at end
        app.handle_config_input(KeyCode::End);
        if let Screen::Config { cursor_position, .. } = &app.screen {
            assert_eq!(*cursor_position, cmd_len);
        }

        // Left -> cursor at end - 1
        app.handle_config_input(KeyCode::Left);
        if let Screen::Config { cursor_position, .. } = &app.screen {
            assert_eq!(*cursor_position, cmd_len - 1);
        }
    }

    #[test]
    fn test_config_save_updates_app_config() {
        let mut app = make_config_app();
        app.push_config_screen();

        // Enter edit mode, modify command
        app.handle_config_input(KeyCode::Enter);
        app.handle_config_input(KeyCode::Home);
        app.handle_config_input(KeyCode::Char('X'));

        // Return to navigation mode
        app.handle_config_input(KeyCode::Esc);

        // Save (S works in navigation mode regardless of focused field)
        app.handle_config_input(KeyCode::Char('S'));
        assert!(matches!(app.screen, Screen::ChangeList { .. }));
        assert_eq!(app.config.command, "Xtest-tool {prompt}");
    }

    #[test]
    fn test_config_reset_to_defaults() {
        let mut app = make_config_app();
        app.push_config_screen();

        // Switch to prompt field
        app.handle_config_input(KeyCode::Tab);

        // Reset to defaults
        app.handle_config_input(KeyCode::Char('D'));
        if let Screen::Config { command, prompt, focused_field, .. } = &app.screen {
            let defaults = TuiConfig::default();
            assert_eq!(command, &defaults.command);
            assert_eq!(prompt, &defaults.prompt);
            assert_eq!(*focused_field, ConfigField::Command);
        } else {
            panic!("Expected Config screen");
        }
    }

    #[test]
    fn test_config_enter_on_prompt_returns_true() {
        let mut app = make_config_app();
        app.push_config_screen();

        // Switch to prompt field
        app.handle_config_input(KeyCode::Tab);

        // Enter on prompt should signal editor
        let result = app.handle_config_input(KeyCode::Enter);
        assert!(result, "Enter on prompt field should return true for editor");
        // Screen should still be Config
        assert!(matches!(app.screen, Screen::Config { .. }));
    }

    #[test]
    fn test_config_enter_on_command_doesnt_signal_editor() {
        let mut app = make_config_app();
        app.push_config_screen();

        // On command field, Enter should not signal editor
        let result = app.handle_config_input(KeyCode::Enter);
        assert!(!result, "Enter on command field should not signal editor");
    }

    #[test]
    fn test_config_s_in_navigation_mode_saves() {
        let mut app = make_config_app();
        app.push_config_screen();

        // S in navigation mode saves and exits, even with Command focused
        app.handle_config_input(KeyCode::Char('S'));
        assert!(matches!(app.screen, Screen::ChangeList { .. }));
    }

    #[test]
    fn test_config_d_in_navigation_mode_resets() {
        let mut app = make_config_app();
        app.push_config_screen();

        // D in navigation mode resets to defaults, even with Command focused
        app.handle_config_input(KeyCode::Char('D'));
        if let Screen::Config { command, prompt, .. } = &app.screen {
            let defaults = TuiConfig::default();
            assert_eq!(command, &defaults.command);
            assert_eq!(prompt, &defaults.prompt);
        } else {
            panic!("Expected Config screen");
        }
    }

    #[test]
    fn test_set_config_prompt() {
        let mut app = make_config_app();
        app.push_config_screen();

        app.set_config_prompt("new prompt text".to_string());
        if let Screen::Config { prompt, .. } = &app.screen {
            assert_eq!(prompt, "new prompt text");
        } else {
            panic!("Expected Config screen");
        }
    }

    #[test]
    fn test_config_char_keys_ignored_in_navigation_mode() {
        let mut app = make_config_app();
        app.push_config_screen();

        let original = if let Screen::Config { command, .. } = &app.screen {
            command.clone()
        } else {
            panic!("Expected Config screen");
        };

        // Character keys should be ignored in navigation mode
        app.handle_config_input(KeyCode::Char('x'));
        app.handle_config_input(KeyCode::Char('y'));
        app.handle_config_input(KeyCode::Char('z'));

        if let Screen::Config { command, editing, .. } = &app.screen {
            assert_eq!(command, &original, "Command should not change in navigation mode");
            assert!(!editing, "Should still be in navigation mode");
        } else {
            panic!("Expected Config screen");
        }
    }

    #[test]
    fn test_config_enter_activates_edit_mode() {
        let mut app = make_config_app();
        app.push_config_screen();

        // Initially not editing
        if let Screen::Config { editing, .. } = &app.screen {
            assert!(!editing);
        }

        // Enter activates edit mode on Command field
        app.handle_config_input(KeyCode::Enter);
        if let Screen::Config { editing, .. } = &app.screen {
            assert!(*editing, "Enter should activate edit mode");
        } else {
            panic!("Expected Config screen");
        }
    }

    #[test]
    fn test_config_esc_exits_edit_mode() {
        let mut app = make_config_app();
        app.push_config_screen();

        // Enter edit mode
        app.handle_config_input(KeyCode::Enter);
        // Type something
        app.handle_config_input(KeyCode::Char('X'));

        // Esc should exit edit mode (not the config screen)
        app.handle_config_input(KeyCode::Esc);
        if let Screen::Config { editing, command, .. } = &app.screen {
            assert!(!editing, "Esc should return to navigation mode");
            assert!(command.contains('X'), "Edits should be preserved");
        } else {
            panic!("Expected Config screen, not exit");
        }
    }

    #[test]
    fn test_config_enter_exits_edit_mode() {
        let mut app = make_config_app();
        app.push_config_screen();

        // Enter edit mode
        app.handle_config_input(KeyCode::Enter);
        // Type something
        app.handle_config_input(KeyCode::Char('Z'));

        // Enter again should exit edit mode
        app.handle_config_input(KeyCode::Enter);
        if let Screen::Config { editing, command, .. } = &app.screen {
            assert!(!editing, "Enter should return to navigation mode");
            assert!(command.contains('Z'), "Edits should be preserved");
        } else {
            panic!("Expected Config screen");
        }
    }

    #[test]
    fn test_config_backspace_at_start_noop() {
        let mut app = make_config_app();
        app.push_config_screen();

        // Enter edit mode, move to start
        app.handle_config_input(KeyCode::Enter);
        app.handle_config_input(KeyCode::Home);
        let original = if let Screen::Config { command, .. } = &app.screen {
            command.clone()
        } else {
            String::new()
        };

        app.handle_config_input(KeyCode::Backspace);
        if let Screen::Config { command, cursor_position, .. } = &app.screen {
            assert_eq!(command, &original);
            assert_eq!(*cursor_position, 0);
        }
    }

    #[test]
    fn test_config_delete_at_end_noop() {
        let mut app = make_config_app();
        app.push_config_screen();

        // Enter edit mode (cursor starts at end)
        app.handle_config_input(KeyCode::Enter);
        let original = if let Screen::Config { command, .. } = &app.screen {
            command.clone()
        } else {
            String::new()
        };

        app.handle_config_input(KeyCode::Delete);
        if let Screen::Config { command, .. } = &app.screen {
            assert_eq!(command, &original);
        }
    }
}
