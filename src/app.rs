use crossterm::event::KeyCode;
use std::path::PathBuf;

use crate::data::{self, ChangeEntry, ChangeStatusOutput};
#[cfg(test)]
use crate::data::ArtifactStatus;

#[derive(Debug, Clone)]
pub enum Screen {
    ChangeList {
        changes: Vec<ChangeEntry>,
        selected: usize,
        error: Option<String>,
    },
    ArtifactMenu {
        change_name: String,
        change_dir: PathBuf,
        items: Vec<ArtifactMenuItem>,
        selected: usize,
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
}

impl App {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let screen = match data::list_changes() {
            Ok(list) => Screen::ChangeList {
                changes: list.changes,
                selected: 0,
                error: None,
            },
            Err(e) => Screen::ChangeList {
                changes: Vec::new(),
                selected: 0,
                error: Some(e),
            },
        };

        Ok(App {
            screen,
            screen_stack: Vec::new(),
            should_quit: false,
        })
    }

    pub fn handle_change_list_input(&mut self, key: KeyCode) {
        let Screen::ChangeList {
            changes, selected, ..
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
            KeyCode::Enter => {
                if changes.is_empty() {
                    return;
                }
                let change = &changes[*selected];
                let change_name = change.name.clone();
                self.enter_artifact_menu(&change_name);
            }
            _ => {}
        }
    }

    fn enter_artifact_menu(&mut self, change_name: &str) {
        let status = match data::get_change_status(change_name) {
            Ok(s) => s,
            Err(_) => return,
        };

        let change_dir = self.find_change_dir(change_name);
        let items = build_artifact_menu_items(&status, &change_dir);

        let old_screen = std::mem::replace(
            &mut self.screen,
            Screen::ArtifactMenu {
                change_name: change_name.to_string(),
                change_dir,
                items,
                selected: 0,
            },
        );
        self.screen_stack.push(old_screen);
    }

    fn find_change_dir(&self, change_name: &str) -> PathBuf {
        // Try to find the openspec change directory
        let cwd = std::env::current_dir().unwrap_or_default();
        cwd.join("openspec").join("changes").join(change_name)
    }

    pub fn handle_artifact_menu_input(&mut self, key: KeyCode) {
        let Screen::ArtifactMenu {
            items,
            selected,
            change_dir,
            ..
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
            },
            screen_stack: Vec::new(),
            should_quit: false,
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
        };

        let mut app = App {
            screen: Screen::ArtifactMenu {
                change_name: "test".to_string(),
                change_dir: PathBuf::from("/tmp"),
                items: vec![],
                selected: 0,
            },
            screen_stack: vec![original_screen],
            should_quit: false,
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
        };

        let mut app = App {
            screen: Screen::ArtifactView {
                title: "Proposal".to_string(),
                content: "hello\nworld".to_string(),
                scroll: 0,
            },
            screen_stack: vec![menu_screen],
            should_quit: false,
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
            },
            screen_stack: Vec::new(),
            should_quit: false,
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
            },
            screen_stack: Vec::new(),
            should_quit: false,
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
            },
            screen_stack: Vec::new(),
            should_quit: false,
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
            },
            screen_stack: Vec::new(),
            should_quit: false,
        };

        app.handle_artifact_menu_input(KeyCode::Enter);
        assert!(matches!(app.screen, Screen::ArtifactMenu { .. }));
    }

    #[test]
    fn test_change_list_empty_navigation() {
        let mut app = App {
            screen: Screen::ChangeList {
                changes: vec![],
                selected: 0,
                error: None,
            },
            screen_stack: Vec::new(),
            should_quit: false,
        };

        // Navigation on empty list shouldn't panic
        app.handle_change_list_input(KeyCode::Down);
        app.handle_change_list_input(KeyCode::Up);
        app.handle_change_list_input(KeyCode::Enter);
        assert!(matches!(app.screen, Screen::ChangeList { .. }));
    }
}
