use crossterm::event::KeyCode;
use std::path::PathBuf;

use crate::data::{self, ChangeEntry, ChangeStatusOutput};
use crate::runner::{self, stop_implementation, ImplState};
#[cfg(test)]
use crate::data::ArtifactStatus;

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeTab {
    Active,
    Archived,
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

        Ok(App {
            screen,
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
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
                    let change_dir = change_dir.clone();
                    let old_screen = std::mem::replace(
                        &mut self.screen,
                        Screen::ArtifactView {
                            title,
                            content,
                            scroll: 0,
                        },
                    );
                    let _ = change_dir;
                    self.screen_stack.push(old_screen);
                }
            }
            KeyCode::Char('R') => {
                if !*is_archived && self.implementation.is_none() {
                    let name = change_name.clone();
                    self.implementation = Some(runner::start_implementation(&name));
                }
            }
            KeyCode::Esc => {
                if let Some(prev) = self.screen_stack.pop() {
                    self.screen = prev;
                }
            }
            _ => {}
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
            },
            screen_stack: vec![menu_screen],
            should_quit: false,
            implementation: None,
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
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: None,
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
            },
            screen_stack: Vec::new(),
            should_quit: false,
            implementation: Some(existing_impl),
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
        };

        let dir = app.find_change_dir("2026-03-06-my-change", true);
        assert!(dir.ends_with("openspec/changes/archive/2026-03-06-my-change"));
    }
}
